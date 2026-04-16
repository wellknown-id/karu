#[cfg(all(feature = "wasm", feature = "cedar"))]
mod tests {
    use wasm_bindgen_test::*;
    use karu::wasm::karu_eval_cedar_js;
    use wasm_bindgen::JsValue;

    // wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_karu_eval_cedar_js_invalid_cedar_policy() {
        let invalid_cedar = "permit(principal, action, resource"; // syntax error
        let input = r#"{"principal": "alice", "action": "read", "resource": "file"}"#;

        let result = karu_eval_cedar_js(invalid_cedar, input);

        let obj = js_sys::Object::from(result);
        let error_key = JsValue::from_str("error");
        let has_error = js_sys::Reflect::has(&obj, &error_key).unwrap();
        assert!(has_error, "Result should contain an 'error' key");

        let error_val = js_sys::Reflect::get(&obj, &error_key).unwrap();
        let error_str = error_val.as_string().unwrap();
        assert!(
            error_str.starts_with("Cedar import error:"),
            "Error should start with 'Cedar import error:', got: '{}'",
            error_str
        );
    }

    #[wasm_bindgen_test]
    fn test_karu_eval_cedar_js_invalid_json() {
        let valid_cedar = "permit(principal, action, resource);";
        let invalid_input = r#"{principal: "alice", "action": "read", "resource": "file"}"#;

        let result = karu_eval_cedar_js(valid_cedar, invalid_input);

        let obj = js_sys::Object::from(result);
        let error_key = JsValue::from_str("error");
        let has_error = js_sys::Reflect::has(&obj, &error_key).unwrap();
        assert!(has_error, "Result should contain an 'error' key");

        let error_val = js_sys::Reflect::get(&obj, &error_key).unwrap();
        let error_str = error_val.as_string().unwrap();
        assert!(
            error_str.starts_with("Invalid JSON:"),
            "Error should start with 'Invalid JSON:', got: '{}'",
            error_str
        );
    }
}
