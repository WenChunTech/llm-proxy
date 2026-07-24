use llm_proxy::config::{parse_get_result, parse_set_result};

#[test]
fn parses_direct_get_null() {
    let value = parse_get_result(r#"{"result":null}"#).unwrap();
    assert!(value.is_none());
}

#[test]
fn parses_pipeline_get_null() {
    let value = parse_get_result(r#"[{"result":null}]"#).unwrap();
    assert!(value.is_none());
}

#[test]
fn parses_pipeline_get_string() {
    let value = parse_get_result(r#"[{"result":"{\"port\":1}"}]"#).unwrap();
    assert_eq!(value.as_deref(), Some(r#"{"port":1}"#));
}

#[test]
fn parses_direct_get_string() {
    let value = parse_get_result(r#"{"result":"{\"port\":2}"}"#).unwrap();
    assert_eq!(value.as_deref(), Some(r#"{"port":2}"#));
}

#[test]
fn parses_get_error() {
    let err = parse_get_result(r#"{"error":"WRONGPASS invalid password"}"#).unwrap_err();
    assert!(err.to_string().contains("WRONGPASS"));
}

#[test]
fn parses_pipeline_set_ok() {
    parse_set_result(r#"[{"result":"OK"}]"#).unwrap();
}

#[test]
fn parses_direct_set_ok() {
    parse_set_result(r#"{"result":"OK"}"#).unwrap();
}

#[test]
fn parses_set_error() {
    let err = parse_set_result(r#"{"error":"ERR wrong number of arguments for 'set' command"}"#)
        .unwrap_err();
    assert!(err.to_string().contains("wrong number of arguments"));
}

#[test]
fn parses_pipeline_mixed_uses_first_command() {
    // Pipeline may contain per-command errors; we take the first entry.
    let err = parse_get_result(r#"[{"error":"ERR boom"},{"result":"x"}]"#).unwrap_err();
    assert!(err.to_string().contains("ERR boom"));
}
