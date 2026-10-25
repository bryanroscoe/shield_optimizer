#[derive(Debug, PartialEq, Eq)]
pub enum TransportSelection {
    Subprocess,
    Direct,
}

pub fn parse_selection(value: Option<&str>) -> Result<TransportSelection, String> {
    match value {
        None | Some("subprocess") => Ok(TransportSelection::Subprocess),
        Some("direct") => Ok(TransportSelection::Direct),
        Some(_) => Err("SHIELD_OPTIMIZER_TRANSPORT must be subprocess or direct.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_transport_requires_explicit_selection() {
        assert_eq!(
            parse_selection(None).unwrap(),
            TransportSelection::Subprocess
        );
        assert_eq!(
            parse_selection(Some("subprocess")).unwrap(),
            TransportSelection::Subprocess
        );
        assert_eq!(
            parse_selection(Some("direct")).unwrap(),
            TransportSelection::Direct
        );
        assert!(parse_selection(Some("automatic")).is_err());
    }
    #[test]
    fn desktop_bundle_retains_the_vendored_mit_notice() {
        assert_eq!(
            include_str!("../../resources/direct-adb-MIT-LICENSE.txt"),
            include_str!("../../../vendor/adb_client/LICENSE")
        );
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert!(config["bundle"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "resources/direct-adb-MIT-LICENSE.txt"));
    }
}
