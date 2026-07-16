use super::*;

#[test]
fn creates_dock_when_config_is_empty() {
    let output = ensure_dock_json(None, DOCK_TITLE, DOCK_URL, DOCK_UUID).unwrap().unwrap();
    let docks: Vec<Value> = serde_json::from_str(&output).unwrap();

    assert_eq!(docks.len(), 1);
    assert_eq!(docks[0]["title"], DOCK_TITLE);
    assert_eq!(docks[0]["url"], DOCK_URL);
    assert_eq!(docks[0]["uuid"], DOCK_UUID);
}

#[test]
fn leaves_existing_matching_dock_unchanged() {
    let existing = r#"[{"title":"The Golden Eye","url":"http://127.0.0.1:31337/","uuid":"thegoldeneyedashboard"}]"#;

    assert_eq!(ensure_dock_json(Some(existing), DOCK_TITLE, DOCK_URL, DOCK_UUID).unwrap(), None);
}

#[test]
fn preserves_existing_docks_when_appending() {
    let existing = r#"[{"title":"Other","url":"http://localhost:1234/","uuid":"other","extra":true}]"#;
    let output = ensure_dock_json(Some(existing), DOCK_TITLE, DOCK_URL, DOCK_UUID).unwrap().unwrap();
    let docks: Vec<Value> = serde_json::from_str(&output).unwrap();

    assert_eq!(docks.len(), 2);
    assert_eq!(docks[0]["extra"], true);
    assert_eq!(docks[1]["title"], DOCK_TITLE);
}

#[test]
fn rejects_malformed_existing_config() {
    let error = ensure_dock_json(Some("{not json"), DOCK_TITLE, DOCK_URL, DOCK_UUID).unwrap_err();

    assert!(error.contains("could not parse"));
}
