use super::*;

#[test]
fn creates_dock_when_config_is_empty() {
    let default_url = crate::config::browser_dock_url();
    let output = ensure_dock_json(None, DOCK_TITLE, &default_url, DOCK_UUID).unwrap().unwrap();
    let docks: Vec<Value> = serde_json::from_str(&output).unwrap();

    assert_eq!(docks.len(), 1);
    assert_eq!(docks[0]["title"], DOCK_TITLE);
    assert_eq!(docks[0]["url"], default_url);
    assert_eq!(docks[0]["uuid"], DOCK_UUID);
}

#[test]
fn leaves_existing_matching_dock_unchanged() {
    let default_url = crate::config::browser_dock_url();
    let existing = format!(r#"[{{"title":"The Golden Eye","url":"{default_url}","uuid":"thegoldeneyedashboard"}}]"#);

    assert_eq!(ensure_dock_json(Some(&existing), DOCK_TITLE, &default_url, DOCK_UUID).unwrap(), None);
}

#[test]
fn preserves_existing_docks_when_appending() {
    let default_url = crate::config::browser_dock_url();
    let existing = r#"[{"title":"Other","url":"http://localhost:1234/","uuid":"other","extra":true}]"#;
    let output = ensure_dock_json(Some(existing), DOCK_TITLE, &default_url, DOCK_UUID).unwrap().unwrap();
    let docks: Vec<Value> = serde_json::from_str(&output).unwrap();

    assert_eq!(docks.len(), 2);
    assert_eq!(docks[0]["extra"], true);
    assert_eq!(docks[1]["title"], DOCK_TITLE);
}

#[test]
fn rejects_malformed_existing_config() {
    let default_url = crate::config::browser_dock_url();
    let error = ensure_dock_json(Some("{not json"), DOCK_TITLE, &default_url, DOCK_UUID).unwrap_err();

    assert!(error.contains("could not parse"));
}
