//! Launchable apps for the Remote's Apps key: which installed apps the TV can
//! open, by what name, and the command that opens one. Pure: the command layer
//! runs the queries and hands their output here.

use serde::Serialize;

use super::launcher::is_valid_package_name;

/// The category TV apps put on the activity that gets their Home screen tile.
pub const LEANBACK_LAUNCHER: &str = "android.intent.category.LEANBACK_LAUNCHER";
/// The phone launcher category. Fire TV apps and some sideloads carry only this.
pub const LAUNCHER: &str = "android.intent.category.LAUNCHER";

/// The read that lists every enabled activity of `category` for user 0, one
/// `package/activity` component per line.
pub fn launch_query(category: &str) -> String {
    format!(
        "cmd package query-activities --components --user 0 -a android.intent.action.MAIN -c {category}"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchableApp {
    pub package: String,
    /// From the catalog or the known-names list. None when neither knows the
    /// app; the apps then show its package id.
    pub name: Option<String>,
    /// True when the app has a TV (Leanback) launch activity.
    pub tv: bool,
}

/// Packages named by `--components` output, in order, each once. Lines that
/// aren't a `package/activity` component, or name an invalid package, are
/// skipped.
pub fn parse_launch_components(stdout: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in stdout.lines().map(str::trim) {
        let Some((pkg, activity)) = line.split_once('/') else {
            continue;
        };
        if activity.is_empty() || !is_valid_package_name(pkg) {
            continue;
        }
        if !out.iter().any(|p| p == pkg) {
            out.push(pkg.to_string());
        }
    }
    out
}

/// Every app with a TV or phone launch activity, once, named where a list
/// knows it. Named apps come first, by name; unnamed ones follow by package.
pub fn launchable_apps(
    leanback: &str,
    launcher: &str,
    name_of: impl Fn(&str) -> Option<String>,
) -> Vec<LaunchableApp> {
    let tv = parse_launch_components(leanback);
    let mut packages = tv.clone();
    for pkg in parse_launch_components(launcher) {
        if !packages.contains(&pkg) {
            packages.push(pkg);
        }
    }
    let mut apps: Vec<LaunchableApp> = packages
        .into_iter()
        .map(|package| LaunchableApp {
            name: name_of(&package),
            tv: tv.contains(&package),
            package,
        })
        .collect();
    apps.sort_by(|a, b| match (&a.name, &b.name) {
        (Some(x), Some(y)) => x
            .to_lowercase()
            .cmp(&y.to_lowercase())
            .then_with(|| a.package.cmp(&b.package)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.package.cmp(&b.package),
    });
    apps
}

/// `launch_query` narrowed to one package, or None for anything that isn't a
/// package name (it is interpolated into a shell command).
pub fn launch_query_for(category: &str, package: &str) -> Option<String> {
    (is_launcher_category(category) && is_valid_package_name(package))
        .then(|| format!("{} {package}", launch_query(category)))
}

fn is_launcher_category(category: &str) -> bool {
    category == LEANBACK_LAUNCHER || category == LAUNCHER
}

/// An activity name as `--components` prints it (`.Main` or `com.x.Main`).
/// Nested classes (`$`) are refused: the name goes into a shell command.
fn is_valid_activity(activity: &str) -> bool {
    activity.split('.').all(|seg| {
        seg.is_empty()
            || seg
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    }) && activity
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && activity.chars().any(|c| c.is_ascii_alphanumeric())
        && !activity.ends_with('.')
}

/// The launch activity to open `package` with: its TV one when it has one,
/// else its phone one, taken from those queries' `--components` output.
/// Opening a named component never depends on the activity also declaring
/// CATEGORY_DEFAULT, which an implicit `am start` of the package would.
pub fn pick_component(
    package: &str,
    leanback: &str,
    launcher: &str,
) -> Option<(String, &'static str)> {
    for (out, category) in [(leanback, LEANBACK_LAUNCHER), (launcher, LAUNCHER)] {
        for line in out.lines().map(str::trim) {
            if let Some((pkg, activity)) = line.split_once('/') {
                if pkg == package && is_valid_activity(activity) {
                    return Some((line.to_string(), category));
                }
            }
        }
    }
    None
}

/// The `am start` that opens `component` (`package/activity`) as a launcher
/// would, or None for anything that isn't a valid component and category.
pub fn launch_command(component: &str, category: &str) -> Option<String> {
    let (pkg, activity) = component.split_once('/')?;
    (is_launcher_category(category) && is_valid_package_name(pkg) && is_valid_activity(activity))
        .then(|| format!("am start -n {component} -a android.intent.action.MAIN -c {category}"))
}

/// Whether `am start` reported that it opened something. It prints
/// "Starting: Intent { … }" on success and an "Error:" line (with exit 0 on
/// older builds) when nothing resolved.
pub fn launch_succeeded(exit_ok: bool, combined: &str) -> bool {
    exit_ok
        && combined.contains("Starting:")
        && !combined.contains("Error")
        && !combined.contains("Exception")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn names(pkg: &str) -> Option<String> {
        match pkg {
            "com.netflix.ninja" => Some("Netflix".into()),
            "com.google.android.youtube.tv" => Some("YouTube".into()),
            "org.xbmc.kodi" => Some("kodi".into()),
            _ => None,
        }
    }

    #[test]
    fn parses_components_and_skips_everything_else() {
        let out = "com.netflix.ninja/.MainActivity\n  com.google.android.youtube.tv/com.google.android.apps.youtube.tv.activity.ShellActivity  \n\
                   No activities found\nbad pkg/.X\n1com.bad/.X\ncom.netflix.ninja/.Other\ncom.only.slash/\n";
        assert_eq!(
            parse_launch_components(out),
            vec!["com.netflix.ninja", "com.google.android.youtube.tv"]
        );
        assert!(parse_launch_components("").is_empty());
    }

    #[test]
    fn merges_tv_and_phone_launchers_named_first() {
        let leanback = "com.netflix.ninja/.Main\ncom.example.zeta/.Main\norg.xbmc.kodi/.Splash\n";
        let launcher =
            "org.xbmc.kodi/.Main\ncom.example.alpha/.Main\ncom.google.android.youtube.tv/.Shell\n";
        let apps = launchable_apps(leanback, launcher, names);
        assert_eq!(
            apps.iter().map(|a| a.package.as_str()).collect::<Vec<_>>(),
            vec![
                "org.xbmc.kodi",
                "com.netflix.ninja",
                "com.google.android.youtube.tv",
                "com.example.alpha",
                "com.example.zeta",
            ],
            "named apps by name (case-insensitive), then unnamed by package"
        );
        let kodi = &apps[0];
        assert_eq!(
            (kodi.name.as_deref(), kodi.tv),
            (Some("kodi"), true),
            "listed once, as a TV app"
        );
        let youtube = apps
            .iter()
            .find(|a| a.package == "com.google.android.youtube.tv")
            .unwrap();
        assert!(!youtube.tv, "phone launcher only");
        assert_eq!(apps[3].name, None);
    }

    #[test]
    fn launch_command_accepts_only_components_and_launcher_categories() {
        assert_eq!(
            launch_command("com.netflix.ninja/.MainActivity", LEANBACK_LAUNCHER).as_deref(),
            Some("am start -n com.netflix.ninja/.MainActivity -a android.intent.action.MAIN -c android.intent.category.LEANBACK_LAUNCHER")
        );
        assert!(launch_command("org.xbmc.kodi/org.xbmc.kodi.Splash", LAUNCHER).is_some());
        for bad in [
            "",
            "com.netflix.ninja",
            "com.netflix.ninja/",
            "com.netflix.ninja/.Main$Inner",
            "com.x;reboot/.Main",
            "com.x/.Main;reboot",
            "com.x/.Main $(id)",
            "com.x/-n",
            "com.x/.",
            "com..x/.Main",
        ] {
            assert_eq!(launch_command(bad, LEANBACK_LAUNCHER), None, "{bad}");
        }
        assert_eq!(
            launch_command("com.x/.Main", "android.intent.category.HOME"),
            None
        );
    }

    #[test]
    fn queries_narrow_to_a_valid_package_only() {
        assert_eq!(
            launch_query_for(LEANBACK_LAUNCHER, "com.netflix.ninja").as_deref(),
            Some("cmd package query-activities --components --user 0 -a android.intent.action.MAIN -c android.intent.category.LEANBACK_LAUNCHER com.netflix.ninja")
        );
        assert_eq!(launch_query_for(LAUNCHER, "com.x; reboot"), None);
        assert_eq!(
            launch_query_for("android.intent.category.HOME", "com.x.y"),
            None
        );
    }

    #[test]
    fn picks_the_tv_activity_then_the_phone_one_for_that_package_only() {
        let tv =
            "com.other.app/.Main\ncom.netflix.ninja/.MainActivity\ncom.netflix.ninja/.Second\n";
        assert_eq!(
            pick_component("com.netflix.ninja", tv, "com.netflix.ninja/.Phone\n"),
            Some((
                "com.netflix.ninja/.MainActivity".to_string(),
                LEANBACK_LAUNCHER
            ))
        );
        assert_eq!(
            pick_component(
                "com.example.phone",
                "",
                "com.example.phone/com.example.phone.Main\n"
            ),
            Some((
                "com.example.phone/com.example.phone.Main".to_string(),
                LAUNCHER
            ))
        );
        assert_eq!(pick_component("com.example.gone", tv, ""), None);
        assert_eq!(
            pick_component("com.x.y", "com.x.y/.A$B\ncom.x.y/.Ok\n", ""),
            Some(("com.x.y/.Ok".to_string(), LEANBACK_LAUNCHER)),
            "an unsafe activity name is skipped, not launched"
        );
    }

    #[test]
    fn launch_success_needs_starting_and_no_error() {
        assert!(launch_succeeded(
            true,
            "Starting: Intent { act=android.intent.action.MAIN pkg=com.x }"
        ));
        assert!(!launch_succeeded(
            true,
            "Starting: Intent { … }\nError: Activity not started, unable to resolve Intent { … }"
        ));
        assert!(!launch_succeeded(false, "Starting: Intent { … }"));
        assert!(!launch_succeeded(true, ""));
        assert!(!launch_succeeded(
            true,
            "java.lang.SecurityException: Permission Denial"
        ));
    }
}
