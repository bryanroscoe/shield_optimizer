#!/usr/bin/env python3
"""Seeding, hashing and checks for the rename upgrade tests.

Called by windows.ps1 and macos.sh. Standard library only, so it runs on a
bare runner. Every check appends one JSON line to a results file; `report`
turns that into the job summary and the exit status.

WebView2 keeps localStorage in LevelDB and WKWebView in SQLite. Both are read
here without the browser: a small LevelDB reader (log, table and Snappy
blocks) and the sqlite3 module.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sqlite3
import sys
import tempfile
import time
from pathlib import Path

# Seeded into the old data folder before the old app first runs. The log is
# dated in the past so the old app's own daily log never shares its name.
TELEMETRY = {
    "enabled": False,
    "install_id": "5f0c6a52-6f0e-4c39-9a57-0d7f3f1e2b4a",
    "notice_seen": True,
    "notice_version": 2,
}


def _snapshot(name: str, serial: str, packages: list[str]) -> str:
    return json.dumps(
        {
            "schema_version": 3,
            "saved_at": "2026-10-01T12:00:00Z",
            "label": name,
            "device_name": name,
            "device_serial": serial,
            "device_type": "shield",
            "android_version": "11",
            "disabled_packages": packages,
            "current_launcher": "com.google.android.tvlauncher",
            "settings": {"global.window_animation_scale": "0.5"},
            "absent_settings": [],
        },
        indent=2,
    )


SEEDED = {
    "telemetry.json": json.dumps(TELEMETRY, separators=(",", ":")),
    "snapshots/living-room_20261001-120000.json": _snapshot(
        "living-room", "CI0000000001", ["com.example.bloat", "com.example.ads"]
    ),
    "snapshots/bedroom_20261002-090000.json": _snapshot(
        "bedroom", "CI0000000002", ["com.example.bloat"]
    ),
    "disabled-home-handlers.json": json.dumps(
        {"CI0000000001": ["com.example.launcher"]}, indent=2
    ),
    "native-adb/identity.pkcs8.pem": (
        "-----BEGIN PRIVATE KEY-----\n"
        "Q0kgdXBncmFkZSB0ZXN0IGR1bW15IGtleSwgbm90IGEgcmVhbCBrZXku\n"
        "-----END PRIVATE KEY-----\n"
    ),
    "logs/shield-optimizer.2026-01-15.log": "2026-01-15T10:00:00Z  INFO seeded log line\n",
}

# Where each seeded file should land in the new folder.
RENAMED = {"logs/shield-optimizer.2026-01-15.log": "logs/atv-optimizer.2026-01-15.log"}

TEMP_MARK = ".migrating-"


# --- results ---------------------------------------------------------------


def record(results: str, name: str, status: str, detail: str = "") -> bool:
    line = {"check": name, "status": status, "detail": detail, "time": time.time()}
    with open(results, "a", encoding="utf-8") as f:
        f.write(json.dumps(line) + "\n")
    print(f"[{status.upper():4}] {name}{': ' + detail if detail else ''}", flush=True)
    return status != "fail"


def check(results: str, name: str, ok: bool, detail: str = "", soft: bool = False) -> bool:
    return record(results, name, "ok" if ok else ("warn" if soft else "fail"), detail)


# --- files -----------------------------------------------------------------


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def walk(root: Path):
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        dirnames.sort()
        for name in sorted(filenames):
            yield Path(dirpath) / name


def manifest(root: Path, exclude: list[str]) -> dict:
    out = {}
    if not root.is_dir():
        return out
    for path in walk(root):
        rel = path.relative_to(root).as_posix()
        if any(rel == e or rel.startswith(e.rstrip("/") + "/") for e in exclude):
            continue
        try:
            out[rel] = {"size": path.stat().st_size, "sha256": sha256(path)}
        except OSError as e:
            out[rel] = {"error": str(e)}
    return out


def diff_manifest(before: dict, after: dict) -> list[str]:
    lines = []
    for rel in sorted(set(before) | set(after)):
        if rel not in after:
            lines.append(f"removed {rel}")
        elif rel not in before:
            lines.append(f"added {rel}")
        elif before[rel] != after[rel]:
            lines.append(f"changed {rel}")
    return lines


def cmd_seed(a):
    root = Path(a.data)
    if root.exists() and any(root.iterdir()):
        sys.exit(f"{root} already has content; refusing to seed")
    for rel, body in SEEDED.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(body.encode("utf-8"))
    print(f"seeded {len(SEEDED)} files into {root}")


def cmd_manifest(a):
    m = manifest(Path(a.root), a.exclude or [])
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_text(json.dumps(m, indent=1, sort_keys=True), encoding="utf-8")
    print(f"{len(m)} files under {a.root} -> {a.out}")


def cmd_listing(a):
    lines = []
    for root in a.roots:
        r = Path(root)
        lines.append(f"## {root}")
        if not r.exists():
            lines.append("  (missing)")
            continue
        count = 0
        for dirpath, dirnames, filenames in os.walk(r, followlinks=False):
            dirnames.sort()
            rel = Path(dirpath).relative_to(r).as_posix()
            depth = 0 if rel == "." else rel.count("/") + 1
            if a.max_depth is not None and depth > a.max_depth:
                dirnames[:] = []
                continue
            lines.append(f"  {rel}/")
            for name in sorted(filenames):
                p = Path(dirpath) / name
                try:
                    st = p.lstat()
                    mtime = time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime(st.st_mtime))
                    lines.append(f"    {st.st_size:>10}  {mtime}  {oct(st.st_mode & 0o777)}  {name}")
                except OSError as e:
                    lines.append(f"    ?  {name} ({e})")
                count += 1
                if count > 3000:
                    lines.append("    ... truncated")
                    break
            if count > 3000:
                break
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_text("\n".join(lines) + "\n", encoding="utf-8")


def cmd_check_unchanged(a):
    before = json.loads(Path(a.manifest).read_text(encoding="utf-8"))
    after = manifest(Path(a.root), a.exclude or [])
    diff = diff_manifest(before, after)
    if not before:
        check(a.results, a.name, False, f"the recorded manifest of {a.root} is empty")
        return
    check(
        a.results,
        a.name,
        not diff,
        f"{len(before)} files identical" if not diff else "; ".join(diff[:15]),
    )


def cmd_check_seeded(a):
    old, new = Path(a.old), Path(a.new)
    bad = []
    for rel, body in SEEDED.items():
        dst = new / RENAMED.get(rel, rel)
        if not dst.is_file():
            bad.append(f"missing {dst.relative_to(new).as_posix()}")
        elif dst.read_bytes() != body.encode("utf-8"):
            bad.append(f"differs {dst.relative_to(new).as_posix()}")
        src = old / rel
        if not src.is_file() or src.read_bytes() != body.encode("utf-8"):
            bad.append(f"old copy changed {rel}")
    check(
        a.results,
        a.name,
        not bad,
        f"{len(SEEDED)} seeded files byte-identical" if not bad else "; ".join(bad),
    )


def cmd_check_telemetry(a):
    p = Path(a.new) / "telemetry.json"
    try:
        t = json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        check(a.results, a.name, False, f"{p}: {e}")
        return
    ok = t.get("enabled") is False and t.get("install_id") == TELEMETRY["install_id"]
    check(a.results, a.name, ok, f"enabled={t.get('enabled')} install_id={t.get('install_id')}")


def cmd_check_no_temps(a):
    found = []
    for parent in a.parents:
        p = Path(parent)
        if p.is_dir():
            found += [e.name for e in p.iterdir() if TEMP_MARK in e.name]
    check(a.results, a.name, not found, ", ".join(found) or "none")


# --- logs ------------------------------------------------------------------


def log_lines(dirs: list[str]):
    for d in dirs:
        p = Path(d)
        if not p.is_dir():
            continue
        for f in sorted(p.glob("*.log*")):
            if not f.is_file():
                continue
            try:
                text = f.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            for line in text.splitlines():
                yield f.name, line


def count_matching(dirs, patterns) -> tuple[int, list[str]]:
    regs = [re.compile(p) for p in patterns]
    hits = [f"{n}: {l.strip()}" for n, l in log_lines(dirs) if all(r.search(l) for r in regs)]
    return len(hits), hits


def cmd_count_log(a):
    n, _ = count_matching(a.logs, a.pattern)
    print(n)


def cmd_check_log(a):
    n, hits = count_matching(a.logs, a.pattern)
    ok = n >= a.min and (a.max is None or n <= a.max)
    want = f">= {a.min}" if a.max is None else (f"== {a.min}" if a.min == a.max else f"{a.min}..{a.max}")
    detail = f"{n} matching lines (want {want})"
    if hits:
        detail += " | " + " || ".join(h[:400] for h in hits[:3])
    check(a.results, a.name, ok, detail, soft=a.soft)


# --- LevelDB (WebView2 localStorage) ---------------------------------------

BLOCK = 32768


def varint(buf: bytes, pos: int) -> tuple[int, int]:
    result = shift = 0
    while True:
        b = buf[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        if b < 0x80:
            return result, pos
        shift += 7


def put_varint(n: int) -> bytes:
    out = bytearray()
    while n >= 0x80:
        out.append((n & 0x7F) | 0x80)
        n >>= 7
    out.append(n)
    return bytes(out)


_CRC = []
for _i in range(256):
    _c = _i
    for _ in range(8):
        _c = (_c >> 1) ^ 0x82F63B78 if _c & 1 else _c >> 1
    _CRC.append(_c)


def crc32c(data: bytes) -> int:
    crc = 0xFFFFFFFF
    for b in data:
        crc = _CRC[(crc ^ b) & 0xFF] ^ (crc >> 8)
    return crc ^ 0xFFFFFFFF


def crc_mask(crc: int) -> int:
    return (((crc >> 15) | (crc << 17)) + 0xA282EAD8) & 0xFFFFFFFF


def snappy_decompress(data: bytes) -> bytes:
    size, pos = varint(data, 0)
    out = bytearray()
    while pos < len(data):
        tag = data[pos]
        pos += 1
        kind = tag & 3
        if kind == 0:
            n = tag >> 2
            if n >= 60:
                extra = n - 59
                n = int.from_bytes(data[pos : pos + extra], "little")
                pos += extra
            n += 1
            out += data[pos : pos + n]
            pos += n
            continue
        if kind == 1:
            n = ((tag >> 2) & 7) + 4
            off = ((tag >> 5) << 8) | data[pos]
            pos += 1
        elif kind == 2:
            n = (tag >> 2) + 1
            off = int.from_bytes(data[pos : pos + 2], "little")
            pos += 2
        else:
            n = (tag >> 2) + 1
            off = int.from_bytes(data[pos : pos + 4], "little")
            pos += 4
        if off == 0 or off > len(out):
            raise ValueError("bad snappy offset")
        start = len(out) - off
        for i in range(n):
            out.append(out[start + i])
    if len(out) != size:
        raise ValueError("snappy length mismatch")
    return bytes(out)


def log_records(buf: bytes) -> tuple[list[bytes], int]:
    records, bad, pending, pos = [], 0, None, 0
    while pos < len(buf):
        left = BLOCK - pos % BLOCK
        if left < 7:
            pos += left
            continue
        if pos + 7 > len(buf):
            break
        crc = int.from_bytes(buf[pos : pos + 4], "little")
        n = int.from_bytes(buf[pos + 4 : pos + 6], "little")
        kind = buf[pos + 6]
        if kind == 0 and n == 0:
            pos += left
            continue
        data = buf[pos + 7 : pos + 7 + n]
        pos += 7 + n
        if len(data) < n or crc_mask(crc32c(bytes([kind]) + data)) != crc:
            bad += 1
            pending = None
            continue
        if kind == 1:
            records.append(data)
        elif kind == 2:
            pending = bytearray(data)
        elif kind == 3 and pending is not None:
            pending += data
        elif kind == 4 and pending is not None:
            pending += data
            records.append(bytes(pending))
            pending = None
    return records, bad


def batch_entries(rec: bytes):
    seq = int.from_bytes(rec[0:8], "little")
    count = int.from_bytes(rec[8:12], "little")
    pos = 12
    for i in range(count):
        kind = rec[pos]
        pos += 1
        n, pos = varint(rec, pos)
        key = rec[pos : pos + n]
        pos += n
        value = None
        if kind == 1:
            n, pos = varint(rec, pos)
            value = rec[pos : pos + n]
            pos += n
        yield key, seq + i, kind, value


def block_entries(block: bytes):
    restarts = int.from_bytes(block[-4:], "little")
    end = len(block) - 4 - 4 * restarts
    pos, prev = 0, b""
    while pos < end:
        shared, pos = varint(block, pos)
        unshared, pos = varint(block, pos)
        vlen, pos = varint(block, pos)
        key = prev[:shared] + block[pos : pos + unshared]
        pos += unshared
        value = block[pos : pos + vlen]
        pos += vlen
        prev = key
        yield key, value


def read_block(buf: bytes, handle: bytes) -> bytes:
    off, p = varint(handle, 0)
    size, _ = varint(handle, p)
    data = buf[off : off + size]
    kind = buf[off + size]
    if kind == 0:
        return data
    if kind == 1:
        return snappy_decompress(data)
    raise ValueError(f"unsupported block compression {kind}")


def table_entries(buf: bytes):
    footer = buf[-48:]
    if int.from_bytes(footer[-8:], "little") != 0xDB4775248B80FB57:
        raise ValueError("not a LevelDB table")
    _, p = varint(footer, 0)
    _, p = varint(footer, p)
    index_handle = footer[p:]
    for _, handle in block_entries(read_block(buf, index_handle)):
        for ikey, value in block_entries(read_block(buf, handle)):
            tag = int.from_bytes(ikey[-8:], "little")
            yield ikey[:-8], tag >> 8, tag & 0xFF, value


def leveldb_dir(profile: Path) -> Path | None:
    for cand in [
        profile / "EBWebView" / "Default" / "Local Storage" / "leveldb",
        profile / "Default" / "Local Storage" / "leveldb",
    ]:
        if cand.is_dir():
            return cand
    found = sorted(profile.glob("**/Local Storage/leveldb"))
    return found[0] if found else None


def read_leveldb(db: Path) -> tuple[dict, int, list[str]]:
    """Latest value per key, the highest sequence seen, and notes."""
    latest: dict[bytes, tuple[int, int, bytes | None]] = {}
    max_seq, notes = 0, []

    def take(key, seq, kind, value):
        nonlocal max_seq
        max_seq = max(max_seq, seq)
        if key not in latest or latest[key][0] < seq:
            latest[key] = (seq, kind, value)

    for f in sorted(db.iterdir()):
        try:
            if re.fullmatch(r"\d+\.log", f.name):
                recs, bad = log_records(f.read_bytes())
                notes.append(f"{f.name}: {len(recs)} batches, {bad} bad records")
                for rec in recs:
                    for e in batch_entries(rec):
                        take(*e)
            elif re.fullmatch(r"\d+\.(ldb|sst)", f.name):
                n = 0
                for e in table_entries(f.read_bytes()):
                    take(*e)
                    n += 1
                notes.append(f"{f.name}: {n} entries")
        except Exception as e:  # noqa: BLE001 - report, don't die
            notes.append(f"{f.name}: unreadable ({e})")
    return latest, max_seq, notes


def decode_ls_string(b: bytes) -> str | None:
    if not b:
        return ""
    if b[0] == 0:
        return b[1:].decode("utf-16-le", "replace")
    if b[0] == 1:
        return b[1:].decode("latin-1")
    return None


def leveldb_items(db: Path) -> tuple[dict, list[str]]:
    latest, _, notes = read_leveldb(db)
    items: dict[str, dict[str, str]] = {}
    for key, (_, kind, value) in latest.items():
        if kind != 1 or not key.startswith(b"_"):
            continue
        cut = key.find(b"\x00")
        if cut < 0:
            continue
        origin = key[1:cut].decode("latin-1")
        k = decode_ls_string(key[cut + 1 :])
        v = decode_ls_string(value or b"")
        if k is not None:
            items.setdefault(origin, {})[k] = v
    return items, notes


def leveldb_inject(db: Path, key: str, value: str) -> str:
    latest, max_seq, _ = read_leveldb(db)
    origins = set()
    for k, (_, kind, _v) in latest.items():
        cut = k.find(b"\x00")
        if kind == 1 and k.startswith(b"_") and cut > 0 and b"shieldopt." in k[cut:]:
            origins.add(k[1:cut])
    if len(origins) != 1:
        raise SystemExit(f"expected one origin with shieldopt.* keys, found {sorted(origins)}")
    origin = origins.pop()
    k = b"_" + origin + b"\x00\x01" + key.encode("latin-1")
    v = b"\x01" + value.encode("latin-1")
    seq = max_seq + 1000
    payload = (
        seq.to_bytes(8, "little")
        + (1).to_bytes(4, "little")
        + b"\x01"
        + put_varint(len(k))
        + k
        + put_varint(len(v))
        + v
    )
    logs = sorted(
        (f for f in db.iterdir() if re.fullmatch(r"\d+\.log", f.name)),
        key=lambda f: int(f.stem),
    )
    if not logs:
        raise SystemExit(f"no LevelDB log in {db}")
    path = logs[-1]
    pos = path.stat().st_size
    out, first, rest = bytearray(), True, payload
    while True:
        left = BLOCK - pos % BLOCK
        if left < 7:
            out += b"\x00" * left
            pos += left
            left = BLOCK
        frag, rest = rest[: left - 7], rest[left - 7 :]
        last = not rest
        kind = 1 if first and last else 2 if first else 4 if last else 3
        out += crc_mask(crc32c(bytes([kind]) + frag)).to_bytes(4, "little")
        out += len(frag).to_bytes(2, "little") + bytes([kind]) + frag
        pos += 7 + len(frag)
        first = False
        if last:
            break
    with open(path, "ab") as f:
        f.write(out)
    return f"{path.name} origin={origin.decode('latin-1')} seq={seq}"


# --- WebKit (WKWebView localStorage) ---------------------------------------


def webkit_files(profile: Path) -> list[Path]:
    files = sorted(profile.glob("**/LocalStorage/localstorage.sqlite3"))
    files += sorted(profile.glob("**/LocalStorage/*.localstorage"))
    return files


def webkit_items(profile: Path) -> tuple[dict, list[str]]:
    items, notes = {}, []
    for db in webkit_files(profile):
        rel = db.relative_to(profile).as_posix()
        with tempfile.TemporaryDirectory() as tmp:
            for suffix in ("", "-wal", "-shm"):
                src = Path(str(db) + suffix)
                if src.exists():
                    shutil.copy2(src, Path(tmp) / (db.name + suffix))
            try:
                con = sqlite3.connect(Path(tmp) / db.name)
                rows = con.execute("SELECT key, value FROM ItemTable").fetchall()
                con.close()
            except sqlite3.Error as e:
                notes.append(f"{rel}: unreadable ({e})")
                continue
        vals = {}
        for k, v in rows:
            if isinstance(v, bytes):
                v = v.decode("utf-16-le", "replace")
            vals[k] = v
        items[rel] = vals
        notes.append(f"{rel}: {len(rows)} keys")
    return items, notes


def webkit_inject(profile: Path, key: str, value: str) -> str:
    targets = []
    for db in webkit_files(profile):
        con = sqlite3.connect(db)
        keys = [r[0] for r in con.execute("SELECT key FROM ItemTable")]
        con.close()
        if any(k.startswith("shieldopt.") for k in keys):
            targets.append(db)
    if len(targets) != 1:
        raise SystemExit(f"expected one localStorage database with shieldopt.* keys, found {targets}")
    con = sqlite3.connect(targets[0])
    with con:
        con.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
            (key, value.encode("utf-16-le")),
        )
    con.close()
    return targets[0].relative_to(profile).as_posix()


# --- localStorage commands -------------------------------------------------


def ls_items(kind: str, profile: Path) -> tuple[dict, list[str]]:
    if kind == "webkit":
        return webkit_items(profile)
    db = leveldb_dir(profile)
    if db is None:
        return {}, [f"no Local Storage/leveldb under {profile}"]
    return leveldb_items(db)


def ls_lookup(items: dict, key: str) -> list[str]:
    return [vals[key] for vals in items.values() if key in vals]


def cmd_ls_dump(a):
    items, notes = ls_items(a.kind, Path(a.profile))
    Path(a.out).parent.mkdir(parents=True, exist_ok=True)
    Path(a.out).write_text(json.dumps({"notes": notes, "items": items}, indent=1), encoding="utf-8")
    print("\n".join(notes))


def cmd_ls_inject(a):
    profile = Path(a.profile)
    if a.kind == "webkit":
        where = webkit_inject(profile, a.key, a.value)
    else:
        db = leveldb_dir(profile)
        if db is None:
            raise SystemExit(f"no Local Storage/leveldb under {profile}")
        where = leveldb_inject(db, a.key, a.value)
    # Read it back the same way the checks will.
    items, notes = ls_items(a.kind, profile)
    got = ls_lookup(items, a.key)
    if got != [a.value]:
        raise SystemExit(f"injected {a.key} but read back {got}; {notes}")
    print(f"injected {a.key}={a.value} into {where}")


def cmd_check_ls(a):
    items, notes = ls_items(a.kind, Path(a.profile))
    got = ls_lookup(items, a.key)
    if a.value is not None:
        ok = got == [a.value]
    else:
        ok = len(got) == 1
    detail = f"{a.key} = {got if got else 'absent'}"
    if not ok:
        detail += " | " + "; ".join(notes[:6])
    check(a.results, a.name, ok, detail, soft=a.soft)


# --- report ----------------------------------------------------------------


def cmd_record(a):
    record(a.results, a.name, a.status, a.detail or "")


def cmd_report(a):
    rows = []
    p = Path(a.results)
    if p.exists():
        for line in p.read_text(encoding="utf-8-sig").splitlines():
            line = line.strip()
            if line:
                try:
                    rows.append(json.loads(line))
                except ValueError:
                    rows.append({"check": "unparsable result line", "status": "fail", "detail": line[:200]})
    fails = [r for r in rows if r.get("status") == "fail"]
    warns = [r for r in rows if r.get("status") == "warn"]
    icon = {"ok": "pass", "warn": "warn", "fail": "FAIL"}
    out = [f"### {a.scenario}: {'FAILED' if fails or not rows else 'passed'}", ""]
    out.append(f"{len(rows)} checks, {len(fails)} failed, {len(warns)} warnings.")
    out.append("")
    out.append("| | Check | Detail |")
    out.append("|---|---|---|")
    for r in rows:
        detail = str(r.get("detail", "")).replace("|", "\\|").replace("\n", " ")
        if len(detail) > 600:
            detail = detail[:600] + "..."
        out.append(f"| {icon.get(r.get('status'), r.get('status'))} | {r.get('check')} | {detail} |")
    text = "\n".join(out) + "\n"
    if a.summary:
        with open(a.summary, "a", encoding="utf-8") as f:
            f.write(text + "\n")
    print(text)
    sys.exit(1 if fails or not rows else 0)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    def add(name, fn, results=False, named=False):
        s = sub.add_parser(name)
        s.set_defaults(fn=fn)
        if results:
            s.add_argument("--results", required=True)
        if named:
            s.add_argument("--name", required=True)
        return s

    s = add("seed", cmd_seed)
    s.add_argument("--data", required=True)

    s = add("manifest", cmd_manifest)
    s.add_argument("--root", required=True)
    s.add_argument("--out", required=True)
    s.add_argument("--exclude", action="append")

    s = add("listing", cmd_listing)
    s.add_argument("--out", required=True)
    s.add_argument("--max-depth", type=int)
    s.add_argument("roots", nargs="+")

    s = add("check-unchanged", cmd_check_unchanged, True, True)
    s.add_argument("--manifest", required=True)
    s.add_argument("--root", required=True)
    s.add_argument("--exclude", action="append")

    s = add("check-seeded", cmd_check_seeded, True, True)
    s.add_argument("--old", required=True)
    s.add_argument("--new", required=True)

    s = add("check-telemetry", cmd_check_telemetry, True, True)
    s.add_argument("--new", required=True)

    s = add("check-no-temps", cmd_check_no_temps, True, True)
    s.add_argument("parents", nargs="+")

    s = add("count-log", cmd_count_log)
    s.add_argument("--logs", action="append", required=True)
    s.add_argument("--pattern", action="append", required=True)

    s = add("check-log", cmd_check_log, True, True)
    s.add_argument("--logs", action="append", required=True)
    s.add_argument("--pattern", action="append", required=True)
    s.add_argument("--min", type=int, default=1)
    s.add_argument("--max", type=int)
    s.add_argument("--soft", action="store_true")

    for name, fn in (("ls-dump", cmd_ls_dump), ("ls-inject", cmd_ls_inject), ("check-ls", cmd_check_ls)):
        s = add(name, fn, name == "check-ls", name == "check-ls")
        s.add_argument("--kind", choices=["webview2", "webkit"], required=True)
        s.add_argument("--profile", required=True)
        if name == "ls-dump":
            s.add_argument("--out", required=True)
        else:
            s.add_argument("--key", required=True)
            s.add_argument("--value")
        if name == "check-ls":
            s.add_argument("--soft", action="store_true")

    s = add("record", cmd_record, True, True)
    s.add_argument("--status", choices=["ok", "warn", "fail"], required=True)
    s.add_argument("--detail")

    s = add("report", cmd_report, True)
    s.add_argument("--scenario", required=True)
    s.add_argument("--summary")

    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
