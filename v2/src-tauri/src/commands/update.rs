//! App self-update check — compares the running version against the latest
//! `v2-*` GitHub release and tells the UI whether a newer one exists.

use serde::Serialize;

const RELEASES_API: &str =
    "https://api.github.com/repos/bryanroscoe/shield_optimizer/releases?per_page=20";
// Full releases list, not `/releases/latest` — `latest` redirects to the most
// recent non-prerelease, which is a v1 PowerShell tag (v0.x), so v2 betas never
// show. The list page surfaces every release including v2 pre-releases.
const RELEASES_PAGE: &str = "https://github.com/bryanroscoe/shield_optimizer/releases";

#[derive(Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub update_available: bool,
    pub url: String,
    /// Notes for the version *currently running*, so the app can show what a
    /// user just received after an update installed itself. Distinct from the
    /// pending update's notes, which describe a version not yet installed.
    /// `None` when this build has no matching published release (a dev build)
    /// or GitHub could not be reached.
    pub current_notes: Option<String>,
}

/// `check_for_update` — fetch the latest non-draft `v2-*` release tag from
/// GitHub and compare it to the running version. Network/parse failures
/// return "no update" rather than erroring, so a transient GitHub hiccup
/// never blocks the UI.
#[tauri::command]
pub async fn check_for_update() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    let found = fetch_releases(&current).await.unwrap_or_default();
    let update_available = found
        .latest
        .as_deref()
        .map(|l| is_newer(l, &current))
        .unwrap_or(false);

    Ok(UpdateInfo {
        current,
        latest: found.latest,
        update_available,
        url: RELEASES_PAGE.to_string(),
        current_notes: found.current_notes,
    })
}

#[derive(Default)]
struct ReleaseLookup {
    /// Highest published `v2-*` version.
    latest: Option<String>,
    /// Body of the release matching the running version.
    current_notes: Option<String>,
}

/// GET the releases list once and pull two things out of it: the highest
/// published `v2-<semver>` version, and the notes for the version running
/// right now. One request serves both — the notes are wanted on every launch
/// after an update, including one installed by Homebrew or by replacing the
/// app by hand, neither of which goes through the in-app updater.
async fn fetch_releases(current: &str) -> Result<ReleaseLookup, String> {
    let client = reqwest::Client::builder()
        .user_agent("atv-optimizer-update-check")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }
    // reqwest is built without the `json` feature (rustls-only) — parse the
    // body text ourselves.
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let releases: Vec<serde_json::Value> =
        serde_json::from_str(&body).map_err(|e| e.to_string())?;

    Ok(pick_releases(&releases, current))
}

/// Pure: choose the newest published version and the running version's notes.
/// Split out so the selection is testable without a network round trip.
fn pick_releases(releases: &[serde_json::Value], current: &str) -> ReleaseLookup {
    let mut found = ReleaseLookup::default();
    for r in releases {
        if r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false) {
            continue;
        }
        let Some(tag) = r.get("tag_name").and_then(|t| t.as_str()) else {
            continue;
        };
        let Some(ver) = tag.strip_prefix("v2-") else {
            continue;
        };
        if ver == current {
            found.current_notes = r
                .get("body")
                .and_then(|b| b.as_str())
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .map(str::to_string);
        }
        if found
            .latest
            .as_deref()
            .map(|b| is_newer(ver, b))
            .unwrap_or(true)
        {
            found.latest = Some(ver.to_string());
        }
    }
    found
}

/// Is version `a` newer than version `b`? Compares `MAJOR.MINOR.PATCH` first;
/// on a tie, a stable build (no `-suffix`) beats a pre-release, and otherwise
/// the pre-release identifiers compare lexically (beta.10 > beta.9 by numeric
/// segment). Unparseable inputs are treated as not-newer.
pub fn is_newer(a: &str, b: &str) -> bool {
    compare_versions(a, b) == std::cmp::Ordering::Greater
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a_core, a_pre) = split_pre(a);
    let (b_core, b_pre) = split_pre(b);
    let core = cmp_numeric_triplet(a_core, b_core);
    if core != Ordering::Equal {
        return core;
    }
    match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater, // stable > pre-release
        (Some(_), None) => Ordering::Less,
        (Some(pa), Some(pb)) => cmp_prerelease(pa, pb),
    }
}

fn split_pre(v: &str) -> (&str, Option<&str>) {
    match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    }
}

fn cmp_numeric_triplet(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = parse_triplet(a);
    let pb = parse_triplet(b);
    pa.cmp(&pb)
}

fn parse_triplet(core: &str) -> (u64, u64, u64) {
    let mut it = core.split('.').map(|s| s.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// Compare pre-release strings like `beta.9` vs `beta.10` — split on `.`,
/// compare numeric segments numerically and text segments lexically.
fn cmp_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut ai = a.split('.');
    let mut bi = b.split('.');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(nx), Ok(ny)) => nx.cmp(&ny),
                    _ => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, body: &str) -> serde_json::Value {
        serde_json::json!({ "tag_name": tag, "body": body, "draft": false })
    }

    #[test]
    fn finds_the_notes_for_the_version_actually_running() {
        // After an update installs, the user is running a version we have
        // notes for -- that is what "what's new" shows.
        let releases = vec![
            release("v2-2.2.0", "Newer notes"),
            release("v2-2.1.0", "Notes for the running build"),
            release("v2-2.0.0", "Older notes"),
        ];

        let found = pick_releases(&releases, "2.1.0");

        assert_eq!(found.latest.as_deref(), Some("2.2.0"));
        assert_eq!(
            found.current_notes.as_deref(),
            Some("Notes for the running build")
        );
    }

    #[test]
    fn a_build_with_no_published_release_has_no_notes() {
        // A dev build, or one built from an unreleased commit. Showing another
        // version's notes would be worse than showing none.
        let found = pick_releases(&[release("v2-2.1.0", "Notes")], "2.2.0-dev");

        assert_eq!(found.current_notes, None);
        assert_eq!(found.latest.as_deref(), Some("2.1.0"));
    }

    #[test]
    fn an_empty_release_body_is_not_notes() {
        let found = pick_releases(&[release("v2-2.1.0", "   \n  ")], "2.1.0");

        assert_eq!(found.current_notes, None);
    }

    #[test]
    fn drafts_are_ignored_for_both_answers() {
        let mut draft = release("v2-9.9.9", "Unpublished");
        draft["draft"] = serde_json::Value::Bool(true);
        let releases = vec![draft, release("v2-2.1.0", "Real notes")];

        let found = pick_releases(&releases, "9.9.9");

        assert_eq!(found.latest.as_deref(), Some("2.1.0"));
        assert_eq!(found.current_notes, None, "a draft must not supply notes");
    }

    #[test]
    fn non_v2_tags_are_skipped() {
        // v1 PowerShell releases share this repo and use bare v0.x tags.
        let releases = vec![
            release("v0.9.1", "v1 notes"),
            release("v2-2.1.0", "v2 notes"),
        ];

        let found = pick_releases(&releases, "2.1.0");

        assert_eq!(found.latest.as_deref(), Some("2.1.0"));
        assert_eq!(found.current_notes.as_deref(), Some("v2 notes"));
    }

    #[test]
    fn newer_patch_and_minor() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn stable_beats_prerelease_same_core() {
        assert!(is_newer("0.1.0", "0.1.0-beta.9"));
        assert!(!is_newer("0.1.0-beta.9", "0.1.0"));
    }

    #[test]
    fn prerelease_numeric_segments_compare_numerically() {
        assert!(is_newer("0.1.0-beta.10", "0.1.0-beta.9"));
        assert!(!is_newer("0.1.0-beta.9", "0.1.0-beta.10"));
        assert!(is_newer("0.1.0-rc.1", "0.1.0-beta.9"));
    }

    #[test]
    fn higher_core_beats_any_prerelease() {
        assert!(is_newer("0.2.0-beta.1", "0.1.0"));
    }
}
