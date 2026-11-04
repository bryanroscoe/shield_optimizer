//! Smoke checks for the TCP transport against a real adbd.
//!
//! Run without arguments for usage. Every check prints one `PASS` or `FAIL`
//! line with what the device actually returned, and errors are printed with
//! their variant so a run shows exactly which `RustADBError` real adbd
//! produced. Device-side files go only under `/data/local/tmp/atvopt-devtest-*`
//! and are removed at the end; nothing is installed unless asked, and
//! `uninstall` refuses anything outside `com.atvoptimizer.devtest.`.

use std::{
    fmt::Write as _,
    io::Cursor,
    net::SocketAddr,
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use adb_client::{
    ADBDeviceExt, ADBListItemType, RustADBError, ShellExit,
    tcp::{ADBTcpDevice, pair},
};
use sha2::{Digest, Sha256};

const USAGE: &str = "\
device_smoke: exercise adb_client's TCP transport against a real device.

usage:
  device_smoke probe     <addr> --key <pem> [--auth-timeout <secs>]
  device_smoke suite     <addr> --key <pem> [--auth-timeout <secs>] [--big-mib <n>] [--framebuffer]
  device_smoke sync      <addr> --key <pem> [--big-mib <n>]
  device_smoke pair      <addr> <code> --key <pem>
  device_smoke install   <addr> --key <pem> [--opt <arg>]... [--expect-failure] <apk>...
  device_smoke uninstall <addr> --key <pem> <package>
  device_smoke shell     <addr> --key <pem> <command>
  device_smoke latency   <addr> --key <pem> [--count <n>]

<addr> is ip:port. <pem> is a PKCS#8 PEM RSA key; the device must trust it
(or will prompt for it within --auth-timeout, default 10 s).
With --expect-failure, install passes only on InstallFailed carrying the
package manager's `Failure [...]` reason.
uninstall only accepts packages starting with com.atvoptimizer.devtest.
Exit status: 0 when every check passed, 1 when one failed, 2 for bad usage.
";

const DEVICE_TMP_PREFIX: &str = "/data/local/tmp/atvopt-devtest-";
const UNINSTALL_PREFIX: &str = "com.atvoptimizer.devtest.";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const UTF8_LINE: &str = "é漢字🙂";

struct Args {
    positional: Vec<String>,
    key: Option<PathBuf>,
    auth_timeout: Duration,
    big_mib: usize,
    count: usize,
    framebuffer: bool,
    expect_failure: bool,
    options: Vec<String>,
}

fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut args = Args {
        positional: Vec::new(),
        key: None,
        auth_timeout: Duration::from_secs(10),
        big_mib: 5,
        count: 20,
        framebuffer: false,
        expect_failure: false,
        options: Vec::new(),
    };
    let mut iter = raw.iter();
    while let Some(arg) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--key" => args.key = Some(PathBuf::from(value("--key")?)),
            "--auth-timeout" => {
                let secs: u64 = value("--auth-timeout")?
                    .parse()
                    .map_err(|e| format!("--auth-timeout: {e}"))?;
                args.auth_timeout = Duration::from_secs(secs);
            }
            "--big-mib" => {
                args.big_mib = value("--big-mib")?
                    .parse()
                    .map_err(|e| format!("--big-mib: {e}"))?;
            }
            "--count" => {
                args.count = value("--count")?
                    .parse()
                    .map_err(|e| format!("--count: {e}"))?;
            }
            "--opt" => args.options.push(value("--opt")?),
            "--framebuffer" => args.framebuffer = true,
            "--expect-failure" => args.expect_failure = true,
            _ => args.positional.push(arg.clone()),
        }
    }
    Ok(args)
}

/// Counts results and prints one line per check.
#[derive(Default)]
struct Report {
    passed: usize,
    failed: usize,
}

impl Report {
    fn check(&mut self, name: &str, ok: bool, detail: impl AsRef<str>) -> bool {
        if ok {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        println!(
            "{} {name}: {}",
            if ok { "PASS" } else { "FAIL" },
            detail.as_ref()
        );
        ok
    }

    fn info(name: &str, detail: impl AsRef<str>) {
        println!("INFO {name}: {}", detail.as_ref());
    }

    fn finish(&self) -> ExitCode {
        println!("SUMMARY passed={} failed={}", self.passed, self.failed);
        if self.failed == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        }
    }
}

fn describe(error: &RustADBError) -> String {
    format!("error {error:?} / \"{error}\"")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Deterministic xorshift64* bytes, so the harness needs no RNG dependency.
fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        out.extend_from_slice(&state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
    }
    out.truncate(len);
    out
}

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
        })
}

fn ms(duration: Duration) -> String {
    format!("{:.1}ms", duration.as_secs_f64() * 1000.0)
}

fn parse_addr(text: &str) -> Result<SocketAddr, String> {
    text.parse()
        .map_err(|e| format!("bad address {text:?}: {e}"))
}

fn connect(addr: SocketAddr, args: &Args) -> Result<(ADBTcpDevice, Duration), RustADBError> {
    let key = args.key.clone().unwrap_or_default();
    let started = Instant::now();
    let device =
        ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(addr, key, args.auth_timeout)?;
    Ok((device, started.elapsed()))
}

fn connect_or_fail(report: &mut Report, addr: SocketAddr, args: &Args) -> Option<ADBTcpDevice> {
    let started = Instant::now();
    match connect(addr, args) {
        Ok((device, _)) => Some(device),
        Err(error) => {
            report.check(
                "connect",
                false,
                format!("after {}: {}", ms(started.elapsed()), describe(&error)),
            );
            None
        }
    }
}

struct Output {
    exit: Result<ShellExit, RustADBError>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    elapsed: Duration,
}

fn run(device: &mut ADBTcpDevice, command: &str, timeout: Duration) -> Output {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let started = Instant::now();
    let exit =
        device.shell_command_with_timeout(&command, Some(&mut stdout), Some(&mut stderr), timeout);
    Output {
        exit,
        stdout,
        stderr,
        elapsed: started.elapsed(),
    }
}

fn exit_text(exit: &Result<ShellExit, RustADBError>) -> String {
    match exit {
        Ok(exit) => format!("{exit:?}"),
        Err(error) => describe(error),
    }
}

fn print_connection(device: &ADBTcpDevice, elapsed: Duration) {
    let info = device.connection_info();
    let mut device_features: Vec<_> = info.device_features().iter().cloned().collect();
    device_features.sort();
    let mut shared: Vec<_> = info.features().iter().cloned().collect();
    shared.sort();
    Report::info("connect", format!("handshake {}", ms(elapsed)));
    Report::info(
        "protocol_version",
        format!("{:#010x}", info.protocol_version()),
    );
    Report::info("max_payload", info.max_payload().to_string());
    Report::info("system_type", info.system_type());
    for key in ["ro.product.name", "ro.product.model", "ro.product.device"] {
        Report::info(key, info.property(key).unwrap_or("<absent>"));
    }
    Report::info("device_features", device_features.join(","));
    Report::info("shared_features", shared.join(","));
}

fn cmd_probe(args: &Args) -> Result<ExitCode, String> {
    let [_, addr] = args.positional.as_slice() else {
        return Err("probe <addr> --key <pem>".into());
    };
    let addr = parse_addr(addr)?;
    let mut report = Report::default();
    let started = Instant::now();
    match connect(addr, args) {
        Ok((mut device, elapsed)) => {
            report.check("connect", true, format!("in {}", ms(elapsed)));
            print_connection(&device, elapsed);
            let out = run(&mut device, "echo ok", COMMAND_TIMEOUT);
            report.check(
                "shell.echo",
                matches!(out.exit, Ok(ShellExit::Code(0))) && out.stdout == b"ok\n",
                format!(
                    "{} stdout={:?} in {}",
                    exit_text(&out.exit),
                    String::from_utf8_lossy(&out.stdout),
                    ms(out.elapsed)
                ),
            );
        }
        Err(error) => {
            report.check(
                "connect",
                false,
                format!("after {}: {}", ms(started.elapsed()), describe(&error)),
            );
        }
    }
    Ok(report.finish())
}

fn cmd_pair(args: &Args) -> Result<ExitCode, String> {
    let [_, addr, code] = args.positional.as_slice() else {
        return Err("pair <addr> <code> --key <pem>".into());
    };
    let addr = parse_addr(addr)?;
    let key = args.key.clone().ok_or("pair needs --key")?;
    let mut report = Report::default();
    let started = Instant::now();
    let result = pair(addr, code, &key);
    let elapsed = ms(started.elapsed());
    match result {
        Ok(outcome) => report.check(
            "pair",
            true,
            format!(
                "in {elapsed}: peer kind={} data={:?}",
                outcome.peer_info.kind,
                String::from_utf8_lossy(&outcome.peer_info.data)
            ),
        ),
        Err(error) => report.check(
            "pair",
            false,
            format!("after {elapsed}: error {error:?} / \"{error}\""),
        ),
    };
    Ok(report.finish())
}

fn cmd_install(args: &Args) -> Result<ExitCode, String> {
    let [_, addr, apks @ ..] = args.positional.as_slice() else {
        return Err("install <addr> --key <pem> <apk>...".into());
    };
    if apks.is_empty() {
        return Err("install needs at least one APK".into());
    }
    let addr = parse_addr(addr)?;
    let mut report = Report::default();
    let Some(mut device) = connect_or_fail(&mut report, addr, args) else {
        return Ok(report.finish());
    };
    let options: Vec<&str> = args.options.iter().map(String::as_str).collect();
    Report::info(
        "install.route",
        if apks.len() == 1 {
            "single APK: exec:cmd package install -S"
        } else if device.has_feature("abb_exec") {
            "multiple APKs: abb_exec install-create/write/commit"
        } else {
            "multiple APKs: exec:cmd install-create/write/commit"
        },
    );
    let started = Instant::now();
    let result = device.install_apks(apks, &options, COMMAND_TIMEOUT);
    let elapsed = ms(started.elapsed());
    let name = if args.expect_failure {
        "install.expected_failure"
    } else {
        "install"
    };
    match result {
        Ok(reply) => report.check(
            name,
            !args.expect_failure,
            format!("in {elapsed}: {reply:?}"),
        ),
        Err(error) => {
            let reported =
                matches!(&error, RustADBError::InstallFailed(text) if text.contains("Failure ["));
            report.check(
                name,
                args.expect_failure && reported,
                format!("after {elapsed}: {}", describe(&error)),
            )
        }
    };
    Ok(report.finish())
}

fn cmd_uninstall(args: &Args) -> Result<ExitCode, String> {
    let [_, addr, package] = args.positional.as_slice() else {
        return Err("uninstall <addr> --key <pem> <package>".into());
    };
    if !package.starts_with(UNINSTALL_PREFIX) {
        return Err(format!(
            "refusing to uninstall {package}: not {UNINSTALL_PREFIX}*"
        ));
    }
    let addr = parse_addr(addr)?;
    let mut report = Report::default();
    let Some(mut device) = connect_or_fail(&mut report, addr, args) else {
        return Ok(report.finish());
    };
    match device.uninstall(package, None) {
        Ok(()) => report.check("uninstall", true, package),
        Err(error) => report.check("uninstall", false, describe(&error)),
    };
    Ok(report.finish())
}

fn cmd_shell(args: &Args) -> Result<ExitCode, String> {
    let [_, addr, command] = args.positional.as_slice() else {
        return Err("shell <addr> --key <pem> <command>".into());
    };
    let addr = parse_addr(addr)?;
    let mut report = Report::default();
    let Some(mut device) = connect_or_fail(&mut report, addr, args) else {
        return Ok(report.finish());
    };
    let out = run(&mut device, command, COMMAND_TIMEOUT);
    report.check(
        "shell",
        out.exit.is_ok(),
        format!(
            "{} stdout={:?} stderr={:?} in {}",
            exit_text(&out.exit),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
            ms(out.elapsed)
        ),
    );
    Ok(report.finish())
}

fn latency(report: &mut Report, device: &mut ADBTcpDevice, count: usize) {
    let mut samples = Vec::with_capacity(count);
    let mut ok = true;
    for _ in 0..count {
        let out = run(device, "echo ok", COMMAND_TIMEOUT);
        ok &= matches!(out.exit, Ok(ShellExit::Code(0))) && out.stdout == b"ok\n";
        samples.push(out.elapsed);
    }
    samples.sort();
    let pick = |fraction: f64| {
        let last = samples.len().saturating_sub(1);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let index = ((last as f64) * fraction).round() as usize;
        samples.get(index).copied().unwrap_or_default()
    };
    report.check(
        "latency.echo",
        ok && !samples.is_empty(),
        format!(
            "n={count} min={} median={} p90={} max={}",
            ms(pick(0.0)),
            ms(pick(0.5)),
            ms(pick(0.9)),
            ms(pick(1.0))
        ),
    );
}

fn cmd_latency(args: &Args) -> Result<ExitCode, String> {
    let [_, addr] = args.positional.as_slice() else {
        return Err("latency <addr> --key <pem>".into());
    };
    let addr = parse_addr(addr)?;
    let mut report = Report::default();
    let Some(mut device) = connect_or_fail(&mut report, addr, args) else {
        return Ok(report.finish());
    };
    latency(&mut report, &mut device, args.count);
    Ok(report.finish())
}

fn shell_checks(report: &mut Report, device: &mut ADBTcpDevice) {
    let out = run(device, "echo ok", COMMAND_TIMEOUT);
    report.check(
        "shell.exit_0",
        matches!(out.exit, Ok(ShellExit::Code(0))) && out.stdout == b"ok\n",
        format!(
            "{} stdout={:?}",
            exit_text(&out.exit),
            String::from_utf8_lossy(&out.stdout)
        ),
    );

    let out = run(device, "sh -c 'exit 7'", COMMAND_TIMEOUT);
    report.check(
        "shell.exit_7",
        matches!(out.exit, Ok(ShellExit::Code(7))),
        exit_text(&out.exit),
    );

    let out = run(
        device,
        "sh -c 'printf out; printf err >&2; exit 3'",
        COMMAND_TIMEOUT,
    );
    report.check(
        "shell.stdout_stderr_split",
        matches!(out.exit, Ok(ShellExit::Code(3))) && out.stdout == b"out" && out.stderr == b"err",
        format!(
            "{} stdout={:?} stderr={:?}",
            exit_text(&out.exit),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    );
}

/// Output larger than one ADB packet: multi-byte UTF-8 split across packets,
/// exact bytes, and a real command's full listing.
fn bulk_output_checks(report: &mut Report, device: &mut ADBTcpDevice) {
    let lines = 100_000;
    let expected = format!("{UTF8_LINE}\n").repeat(lines);
    let out = run(
        device,
        &format!("yes '{UTF8_LINE}' | head -n {lines}"),
        COMMAND_TIMEOUT,
    );
    report.check(
        "shell.utf8_multi_packet",
        matches!(out.exit, Ok(ShellExit::Code(0)))
            && out.stdout == expected.as_bytes()
            && std::str::from_utf8(&out.stdout).is_ok(),
        format!(
            "{} bytes={} expected={} sha256_match={} in {}",
            exit_text(&out.exit),
            out.stdout.len(),
            expected.len(),
            sha256_hex(&out.stdout) == sha256_hex(expected.as_bytes()),
            ms(out.elapsed)
        ),
    );

    let count = 200_000;
    let expected: String = (1..=count).fold(String::new(), |mut out, n| {
        let _ = writeln!(out, "{n}");
        out
    });
    let out = run(device, &format!("seq 1 {count}"), COMMAND_TIMEOUT);
    report.check(
        "shell.large_seq",
        matches!(out.exit, Ok(ShellExit::Code(0))) && out.stdout == expected.as_bytes(),
        format!(
            "{} bytes={} expected={} in {}",
            exit_text(&out.exit),
            out.stdout.len(),
            expected.len(),
            ms(out.elapsed)
        ),
    );

    let out = run(device, "pm list packages -f", COMMAND_TIMEOUT);
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines: Vec<&str> = text.lines().collect();
    lines.sort_unstable();
    let well_formed = !lines.is_empty()
        && lines.iter().all(|line| line.starts_with("package:"))
        && text.ends_with('\n');
    report.check(
        "shell.pm_list_packages_f",
        matches!(out.exit, Ok(ShellExit::Code(0))) && well_formed,
        format!(
            "{} lines={} bytes={} sorted_sha256={} in {}",
            exit_text(&out.exit),
            lines.len(),
            out.stdout.len(),
            sha256_hex(format!("{}\n", lines.join("\n")).as_bytes()),
            ms(out.elapsed)
        ),
    );
}

fn sequential_and_timeout_checks(report: &mut Report, device: &mut ADBTcpDevice) {
    let mut sequential_ok = 0;
    let rounds = 30;
    for round in 0..rounds {
        let out = run(device, &format!("echo seq-{round}"), COMMAND_TIMEOUT);
        if matches!(out.exit, Ok(ShellExit::Code(0)))
            && out.stdout == format!("seq-{round}\n").as_bytes()
        {
            sequential_ok += 1;
        }
    }
    report.check(
        "shell.sequential_one_connection",
        sequential_ok == rounds,
        format!("{sequential_ok}/{rounds} correct"),
    );

    let out = run(device, "sleep 2; echo late", Duration::from_millis(500));
    // Unix reports a socket read timeout as EAGAIN (`WouldBlock`), Windows
    // as `TimedOut`.
    let timed_out = matches!(
        &out.exit,
        Err(RustADBError::IOError(e))
            if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)
    );
    let after = run(device, "echo after-timeout", COMMAND_TIMEOUT);
    report.check(
        "shell.timeout_then_reuse",
        timed_out
            && matches!(after.exit, Ok(ShellExit::Code(0)))
            && after.stdout == b"after-timeout\n",
        format!(
            "first={} then={} stdout={:?}",
            exit_text(&out.exit),
            exit_text(&after.exit),
            String::from_utf8_lossy(&after.stdout)
        ),
    );
}

fn is_dir(mode: u32) -> bool {
    mode & 0o170_000 == 0o040_000
}

#[allow(clippy::too_many_lines)]
fn sync_checks(report: &mut Report, device: &mut ADBTcpDevice, base: &str, big_mib: usize) {
    match device.stat(&"/data/local/tmp") {
        Ok(stat) => report.check(
            "sync.stat_dir",
            is_dir(stat.file_perm),
            format!("mode={:o} size={}", stat.file_perm, stat.file_size),
        ),
        Err(error) => report.check("sync.stat_dir", false, describe(&error)),
    };
    match device.list(&"/data/local/tmp") {
        Ok(entries) => report.check("sync.list_dir", true, format!("{} entries", entries.len())),
        Err(error) => report.check("sync.list_dir", false, describe(&error)),
    };

    let big_path = format!("{base}.bin");
    let size = big_mib * 1024 * 1024;
    let data = pseudo_random(size, now_nanos());
    let local_sha = sha256_hex(&data);
    let started = Instant::now();
    let pushed = device.push(&mut Cursor::new(&data), &big_path);
    let push_time = started.elapsed();
    #[allow(clippy::cast_precision_loss)]
    let rate = |elapsed: Duration| size as f64 / 1_048_576.0 / elapsed.as_secs_f64().max(1e-9);
    let pushed_ok = report.check(
        "sync.push_big",
        pushed.is_ok(),
        match &pushed {
            Ok(()) => format!(
                "{size} bytes to {big_path} in {} ({:.1} MiB/s)",
                ms(push_time),
                rate(push_time)
            ),
            Err(error) => describe(error),
        },
    );

    if pushed_ok {
        match device.stat(&big_path) {
            Ok(stat) => report.check(
                "sync.stat_pushed",
                stat.file_size == size as u64 && stat.file_perm & 0o170_000 == 0o100_000,
                format!("mode={:o} size={}", stat.file_perm, stat.file_size),
            ),
            Err(error) => report.check("sync.stat_pushed", false, describe(&error)),
        };

        let out = run(device, &format!("sha256sum {big_path}"), COMMAND_TIMEOUT);
        let device_sha = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        report.check(
            "sync.device_sha256",
            device_sha == local_sha,
            format!("device={device_sha} local={local_sha}"),
        );

        let mut pulled = Vec::new();
        let started = Instant::now();
        let result = device.pull(&big_path, &mut pulled);
        let pull_time = started.elapsed();
        report.check(
            "sync.pull_roundtrip",
            result.is_ok() && sha256_hex(&pulled) == local_sha,
            match &result {
                Ok(()) => format!(
                    "{} bytes sha256_match={} in {} ({:.1} MiB/s)",
                    pulled.len(),
                    sha256_hex(&pulled) == local_sha,
                    ms(pull_time),
                    rate(pull_time)
                ),
                Err(error) => describe(error),
            },
        );

        let name = big_path.rsplit('/').next().unwrap_or_default();
        match device.list(&"/data/local/tmp") {
            Ok(entries) => {
                let found = entries.iter().find_map(|entry| match entry {
                    ADBListItemType::File(item) if item.name == name => Some(item.size),
                    _ => None,
                });
                report.check(
                    "sync.list_contains_pushed",
                    found == Some(size as u64),
                    format!("{name} size={found:?}"),
                )
            }
            Err(error) => report.check("sync.list_contains_pushed", false, describe(&error)),
        };

        let mut under = Vec::new();
        let result = device.pull_with_limit(&big_path, &mut under, size as u64);
        report.check(
            "sync.pull_with_limit_under",
            result.is_ok() && sha256_hex(&under) == local_sha,
            match &result {
                Ok(()) => format!("limit={size} got {} bytes", under.len()),
                Err(error) => describe(error),
            },
        );

        let mut over = Vec::new();
        let limit = size as u64 - 1;
        let result = device.pull_with_limit(&big_path, &mut over, limit);
        report.check(
            "sync.pull_with_limit_over",
            matches!(result, Err(RustADBError::FileTooLarge(got, max)) if got == size as u64 && max == limit)
                && over.is_empty(),
            format!(
                "limit={limit} {} written={}",
                result.as_ref().map_or_else(describe, |()| "Ok".into()),
                over.len()
            ),
        );

        let mut binary = Vec::new();
        let started = Instant::now();
        let result = device.exec_out_with_timeout(
            &format!("cat {big_path}"),
            &mut binary,
            None,
            COMMAND_TIMEOUT,
        );
        report.check(
            "exec_out.binary_cat",
            matches!(result, Ok(ShellExit::Code(0))) && sha256_hex(&binary) == local_sha,
            format!(
                "{} bytes={} sha256_match={} in {}",
                exit_text(&result),
                binary.len(),
                sha256_hex(&binary) == local_sha,
                ms(started.elapsed())
            ),
        );
    }

    let empty_path = format!("{base}-empty");
    let result = device.push(&mut Cursor::new(Vec::<u8>::new()), &empty_path);
    let stat = device.stat(&empty_path);
    report.check(
        "sync.push_empty",
        result.is_ok() && matches!(&stat, Ok(s) if s.file_size == 0),
        format!(
            "push={} stat_size={:?}",
            result.as_ref().map_or_else(describe, |()| "Ok".into()),
            stat.as_ref().map(|s| s.file_size).map_err(describe)
        ),
    );

    let unwritable = format!(
        "/system/{}",
        base.rsplit('/').next().unwrap_or("atvopt-devtest")
    );
    let result = device.push(&mut Cursor::new(b"nope".to_vec()), &unwritable);
    report.check(
        "sync.push_unwritable",
        matches!(result, Err(RustADBError::SyncFailed(_))),
        format!(
            "{unwritable}: {}",
            result
                .as_ref()
                .map_or_else(describe, |()| "Ok (unexpected)".into())
        ),
    );

    let missing = format!("{base}-missing");
    let mut sink = Vec::new();
    let result = device.pull(&missing, &mut sink);
    report.check(
        "sync.pull_missing",
        matches!(result, Err(RustADBError::SyncFailed(_))) && sink.is_empty(),
        result
            .as_ref()
            .map_or_else(describe, |()| "Ok (unexpected)".into()),
    );

    let result = device.stat(&missing);
    report.check(
        "sync.stat_missing",
        result.is_err(),
        result.as_ref().map_or_else(describe, |s| {
            format!(
                "Ok (unexpected) mode={:o} size={}",
                s.file_perm, s.file_size
            )
        }),
    );

    let mut sink = Vec::new();
    let result = device.pull_with_limit(&missing, &mut sink, 1024);
    report.check(
        "sync.pull_with_limit_missing",
        result.is_err() && sink.is_empty(),
        result
            .as_ref()
            .map_or_else(describe, |()| "Ok (unexpected)".into()),
    );

    let out = run(device, "echo still-here", COMMAND_TIMEOUT);
    report.check(
        "sync.connection_intact_after_failures",
        matches!(out.exit, Ok(ShellExit::Code(0))) && out.stdout == b"still-here\n",
        exit_text(&out.exit),
    );
}

fn parallel_check(report: &mut Report, addr: SocketAddr, args: &Args, first: &mut ADBTcpDevice) {
    let second = connect(addr, args);
    let Ok((mut second, elapsed)) = second else {
        let error = second.err().map(|e| describe(&e)).unwrap_or_default();
        report.check("conn.two_at_once", false, error);
        return;
    };
    let rounds = 20;
    let worker = std::thread::spawn(move || {
        let mut ok = 0;
        for round in 0..rounds {
            let out = run(&mut second, &format!("echo b-{round}"), COMMAND_TIMEOUT);
            if matches!(out.exit, Ok(ShellExit::Code(0)))
                && out.stdout == format!("b-{round}\n").as_bytes()
            {
                ok += 1;
            }
        }
        ok
    });
    let mut first_ok = 0;
    for round in 0..rounds {
        let out = run(first, &format!("echo a-{round}"), COMMAND_TIMEOUT);
        if matches!(out.exit, Ok(ShellExit::Code(0)))
            && out.stdout == format!("a-{round}\n").as_bytes()
        {
            first_ok += 1;
        }
    }
    let second_ok = worker.join().unwrap_or(0);
    report.check(
        "conn.two_at_once",
        first_ok == rounds && second_ok == rounds,
        format!(
            "second handshake {}; first {first_ok}/{rounds}, second {second_ok}/{rounds}",
            ms(elapsed)
        ),
    );
}

fn cleanup(report: &mut Report, device: &mut ADBTcpDevice, base: &str) {
    assert!(base.starts_with(DEVICE_TMP_PREFIX));
    let out = run(
        device,
        &format!("rm -f {base}.bin {base}-empty; ls {base}* 2>/dev/null | wc -l"),
        COMMAND_TIMEOUT,
    );
    report.check(
        "cleanup",
        matches!(out.exit, Ok(ShellExit::Code(0))) && out.stdout.trim_ascii() == b"0",
        format!(
            "{} remaining={:?}",
            exit_text(&out.exit),
            String::from_utf8_lossy(out.stdout.trim_ascii())
        ),
    );
}

fn cmd_sync(args: &Args) -> Result<ExitCode, String> {
    let [_, addr] = args.positional.as_slice() else {
        return Err("sync <addr> --key <pem>".into());
    };
    let addr = parse_addr(addr)?;
    let mut report = Report::default();
    let Some(mut device) = connect_or_fail(&mut report, addr, args) else {
        return Ok(report.finish());
    };
    let base = format!(
        "{DEVICE_TMP_PREFIX}{}-{}",
        std::process::id(),
        now_nanos() % 1_000_000
    );
    sync_checks(&mut report, &mut device, &base, args.big_mib);
    cleanup(&mut report, &mut device, &base);
    Ok(report.finish())
}

fn cmd_suite(args: &Args) -> Result<ExitCode, String> {
    let [_, addr] = args.positional.as_slice() else {
        return Err("suite <addr> --key <pem>".into());
    };
    let addr = parse_addr(addr)?;
    let mut report = Report::default();
    let (mut device, elapsed) = match connect(addr, args) {
        Ok(connected) => connected,
        Err(error) => {
            report.check("connect", false, describe(&error));
            return Ok(report.finish());
        }
    };
    report.check("connect", true, format!("in {}", ms(elapsed)));
    print_connection(&device, elapsed);

    let base = format!(
        "{DEVICE_TMP_PREFIX}{}-{}",
        std::process::id(),
        now_nanos() % 1_000_000
    );
    Report::info("device_tmp_base", &base);

    shell_checks(&mut report, &mut device);
    bulk_output_checks(&mut report, &mut device);
    sequential_and_timeout_checks(&mut report, &mut device);
    sync_checks(&mut report, &mut device, &base, args.big_mib);
    parallel_check(&mut report, addr, args, &mut device);
    latency(&mut report, &mut device, args.count);

    #[cfg(feature = "framebuffer")]
    if args.framebuffer {
        let started = Instant::now();
        match device.framebuffer_bytes() {
            Ok(png) => report.check(
                "framebuffer",
                png.starts_with(b"\x89PNG"),
                format!("{} PNG bytes in {}", png.len(), ms(started.elapsed())),
            ),
            Err(error) => report.check("framebuffer", false, describe(&error)),
        };
    }

    cleanup(&mut report, &mut device, &base);
    Ok(report.finish())
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.is_empty() || raw.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let args = match parse_args(&raw) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = match args.positional.first().map(String::as_str) {
        Some("probe") => cmd_probe(&args),
        Some("suite") => cmd_suite(&args),
        Some("sync") => cmd_sync(&args),
        Some("pair") => cmd_pair(&args),
        Some("install") => cmd_install(&args),
        Some("uninstall") => cmd_uninstall(&args),
        Some("shell") => cmd_shell(&args),
        Some("latency") => cmd_latency(&args),
        _ => Err("unknown subcommand".into()),
    };
    result.unwrap_or_else(|message| {
        eprintln!("{message}\n\n{USAGE}");
        ExitCode::from(2)
    })
}
