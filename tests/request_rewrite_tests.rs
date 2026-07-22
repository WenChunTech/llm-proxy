use std::collections::HashSet;

use llm_proxy::provider::types::ProviderType;
use llm_proxy::provider::{
    Providers, has_rewrite, rewrite_request, wire_protocol,
};
use serde_json::{Value, json};

fn tool_type(tool: &Value) -> Option<&str> {
    tool.get("type").and_then(Value::as_str)
}

fn tool_name(tool: &Value) -> Option<&str> {
    tool.get("name").and_then(Value::as_str)
}

#[test]
fn profiles_expose_wire_and_rewrite_flags() {
    assert_eq!(wire_protocol(ProviderType::Grok), ProviderType::Responses);
    assert_eq!(wire_protocol(ProviderType::Codex), ProviderType::Responses);
    assert_eq!(wire_protocol(ProviderType::Gemini), ProviderType::Gemini);
    assert_eq!(wire_protocol(ProviderType::Chat), ProviderType::Chat);
    assert!(has_rewrite(ProviderType::Grok));
    assert!(has_rewrite(ProviderType::Codex));
    assert!(!has_rewrite(ProviderType::Gemini));
    assert!(!has_rewrite(ProviderType::Responses));
    assert!(!has_rewrite(ProviderType::Chat));
    assert!(!has_rewrite(ProviderType::Claude));
}

#[test]
fn skip_convert_and_rewrite_when_same_wire_and_no_dialect() {
    let providers = Providers::new();
    let body = json!({
        "model": "gemini-2.5-pro",
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "keep_me": true
    });

    let out = providers
        .prepare_request(ProviderType::Gemini, body.clone(), ProviderType::Gemini, true)
        .expect("prepare");

    // No convert, no rewrite, Gemini does not force stream.
    assert_eq!(out, body);
    assert!(out.get("stream").is_none());
}

#[test]
fn responses_passthrough_skips_convert_but_sets_stream() {
    let providers = Providers::new();
    let body = json!({
        "model": "gpt",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [{"type": "web_search", "external_web_access": false}]
    });

    let out = providers
        .prepare_request(
            ProviderType::Responses,
            body.clone(),
            ProviderType::Responses,
            true,
        )
        .expect("prepare");

    assert_eq!(out["tools"][0]["external_web_access"], false);
    assert_eq!(out["stream"], true);
    assert_eq!(out["input"], body["input"]);
}

#[test]
fn grok_rewrite_only_when_source_already_responses() {
    let providers = Providers::new();
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "function",
                "name": "read_file",
                "parameters": {"type": "object", "properties": {}}
            },
            {
                "type": "namespace",
                "name": "agent",
                "description": "grouped tools",
                "tools": [
                    {
                        "type": "function",
                        "name": "run_shell",
                        "parameters": {"type": "object"}
                    }
                ]
            },
            {"type": "tool_search"},
            {
                "type": "web_search_preview",
                "external_web_access": false
            },
            {"type": "apply_patch"}
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Grok, body, ProviderType::Responses, true)
        .expect("prepare_request");

    assert_eq!(out["stream"], true);
    let tools = out.get("tools").and_then(Value::as_array).expect("tools");
    let types: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("type").and_then(Value::as_str))
        .collect();
    assert_eq!(types, vec!["function", "function", "web_search", "x_search"]);
    for tool in tools {
        if tool.get("type").and_then(Value::as_str) == Some("function") {
            // function tools are not rewritten for external_web_access
            continue;
        }
        assert!(tool.get("external_web_access").is_none());
    }
}

#[test]
fn grok_skips_dialect_rewrite_when_endpoint_is_not_wire() {
    let providers = Providers::new();
    // Chat endpoint protocol != Grok wire (Responses) → convert only, no rewrite.
    let body = json!({
        "model": "grok-4.5",
        "messages": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "read_file",
                    "parameters": {"type": "object", "properties": {}}
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "apply_patch",
                    "parameters": {"type": "object", "properties": {}}
                }
            }
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Grok, body, ProviderType::Chat, true)
        .expect("prepare_request");

    assert_eq!(out["stream"], true);
    // Converted toward Responses (no Chat `messages` required; stream set by Grok profile).
    let tools = out.get("tools").and_then(Value::as_array).expect("tools");
    // Grok rewrite would inject bare `x_search` whenever tools are present.
    assert!(
        !tools
            .iter()
            .any(|tool| tool.get("type").and_then(Value::as_str) == Some("x_search")),
        "cross-protocol entry must skip grok dialect rewrite: {tools:?}"
    );
}

#[test]
fn gemini_endpoint_to_grok_converts_but_skips_rewrite() {
    let providers = Providers::new();
    let body = json!({
        "model": "gemini-2.5-pro",
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "tools": [{
            "functionDeclarations": [{
                "name": "read_file",
                "description": "read a file",
                "parameters": {"type": "object", "properties": {}}
            }]
        }]
    });

    let out = providers
        .prepare_request(ProviderType::Grok, body, ProviderType::Gemini, true)
        .expect("prepare_request");

    assert_eq!(out["stream"], true);
    // Converted body should no longer be pure Gemini `contents` shape for upstream wire.
    // If convert produced tools, rewrite would inject x_search — assert it did not.
    if let Some(tools) = out.get("tools").and_then(Value::as_array) {
        assert!(
            !tools
                .iter()
                .any(|tool| tool.get("type").and_then(Value::as_str) == Some("x_search")),
            "gemini endpoint + grok must skip dialect rewrite: {tools:?}"
        );
    }
}

#[test]
fn codex_skips_dialect_rewrite_when_endpoint_is_not_wire() {
    let providers = Providers::new();
    let body = json!({
        "model": "codex",
        "messages": [{"role": "user", "content": "hi"}],
        "temperature": 0.5
    });

    let out = providers
        .prepare_request(ProviderType::Codex, body, ProviderType::Chat, false)
        .expect("prepare_request");

    assert_eq!(out["stream"], false);
    // Codex rewrite would strip temperature and force store=false.
    assert_eq!(out.get("temperature"), Some(&json!(0.5)));
    assert!(out.get("store").is_none());
}

#[test]
fn grok_dialect_rewrite_expands_and_filters_tools() {
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "function",
                "name": "read_file",
                "parameters": {"type": "object", "properties": {}}
            },
            {
                "type": "namespace",
                "name": "agent",
                "description": "grouped tools",
                "tools": [
                    {
                        "type": "function",
                        "name": "run_shell",
                        "parameters": {"type": "object"}
                    }
                ]
            },
            {"type": "tool_search"},
            {"type": "web_search_preview"},
            {"type": "apply_patch"}
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    let types: Vec<_> = tools.iter().filter_map(tool_type).collect();
    assert_eq!(types, vec!["function", "function", "web_search", "x_search"]);
    let names: Vec<_> = tools.iter().filter_map(tool_name).collect();
    assert!(names.contains(&"read_file"));
    assert!(names.contains(&"run_shell"));
    assert!(!types.iter().any(|t| *t == "namespace"));
}

#[test]
fn codex_dialect_removes_fields_and_sets_store() {
    let body = json!({
        "model": "codex",
        "temperature": 0.2,
        "max_output_tokens": 128,
        "input": []
    });
    let out = rewrite_request(ProviderType::Codex, body).unwrap();
    assert!(out.get("temperature").is_none());
    assert!(out.get("max_output_tokens").is_none());
    assert_eq!(out["store"], false);
}

#[test]
fn codex_prepare_request_uses_provider_profile() {
    let providers = Providers::new();
    let body = json!({
        "model": "codex",
        "temperature": 0.5,
        "max_output_tokens": 256,
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "function",
                "name": "echo",
                "parameters": {"type": "object"}
            }
        ]
    });

    let out = providers
        .prepare_request(ProviderType::Codex, body, ProviderType::Responses, false)
        .expect("prepare_request");

    assert!(out.get("temperature").is_none());
    assert!(out.get("max_output_tokens").is_none());
    assert_eq!(out["store"], false);
    assert_eq!(out["stream"], false);
    assert_eq!(out["tools"][0]["type"], "function");
}

#[test]
fn rewrite_helpers_via_grok_profile_expand_namespace() {
    let body = json!({
        "model": "grok",
        "tools": [
            {
                "type": "function",
                "name": "read_file",
                "parameters": {"type": "object"}
            },
            {
                "type": "namespace",
                "name": "browser",
                "description": "browser tools",
                "tools": [
                    {
                        "type": "function",
                        "name": "open",
                        "parameters": {"type": "object"}
                    },
                    {
                        "type": "function",
                        "name": "click",
                        "parameters": {"type": "object"}
                    }
                ]
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    // expanded nested tools + injected x_search
    assert_eq!(tools.len(), 4);
    assert!(tools.iter().all(|t| tool_type(t) != Some("namespace")));
    let names: HashSet<_> = tools.iter().filter_map(tool_name).collect();
    assert_eq!(names, HashSet::from(["read_file", "open", "click"]));
}

#[test]
fn rewrite_helpers_via_grok_profile_rename_on_conflict() {
    let body = json!({
        "tools": [
            {
                "type": "function",
                "name": "open",
                "parameters": {}
            },
            {
                "type": "namespace",
                "name": "browser",
                "tools": [
                    {
                        "type": "function",
                        "name": "open",
                        "parameters": {}
                    }
                ]
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let names: Vec<_> = out
        .get("tools")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(tool_name)
        .collect();
    assert_eq!(names, vec!["open", "browser__open"]);
}

#[test]
fn rewrite_helpers_via_grok_profile_externally_tagged_nested_tools() {
    let body = json!({
        "tools": [
            {
                "type": "namespace",
                "name": "ns",
                "tools": [
                    {
                        "Function": {
                            "name": "do_work",
                            "parameters": {},
                            "strict": null
                        }
                    }
                ]
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tool = &out["tools"][0];
    assert_eq!(tool["type"], "function");
    assert_eq!(tool["name"], "do_work");
}

#[test]
fn grok_profile_adapts_openai_web_search_to_xai_responses_shape() {
    // Matches https://docs.x.ai/developers/tools/web-search Responses examples:
    // domains under filters; image flags top-level; no Codex/OpenAI extras.
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "web_search",
                "external_web_access": false,
                "search_content_types": ["text"],
                "search_context_size": "medium",
                "user_location": {"type": "approximate", "country": "US"},
                "filters": {
                    "allowed_domains": [
                        "a.com", "b.com", "c.com", "d.com", "e.com", "f.com"
                    ],
                    "extra_filter": true
                },
                "enable_image_search": true
            },
            {
                "type": "web_search_preview",
                "external_web_access": true,
                "search_context_size": "high",
                // top-level domains (non-Responses client style) -> folded into filters
                "excluded_domains": ["spam.test", "ads.test"],
                "enable_image_understanding": false
            },
            {
                // both lists present: keep allowed_domains only
                "type": "web_search",
                "filters": {
                    "allowed_domains": ["keep.me"],
                    "excluded_domains": ["drop.me"]
                }
            },
            {
                "type": "function",
                "name": "echo",
                "parameters": {"type": "object"},
                "external_web_access": false
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    // 3 web_search + 1 function + injected bare x_search
    assert_eq!(tools.len(), 5);

    let web0 = &tools[0];
    assert_eq!(web0["type"], "web_search");
    assert!(web0.get("external_web_access").is_none());
    assert!(web0.get("search_content_types").is_none());
    assert!(web0.get("search_context_size").is_none());
    assert!(web0.get("user_location").is_none());
    assert!(web0.get("allowed_domains").is_none());
    assert_eq!(
        web0["filters"]["allowed_domains"],
        json!(["a.com", "b.com", "c.com", "d.com", "e.com"])
    );
    assert!(web0["filters"].get("extra_filter").is_none());
    assert_eq!(web0["enable_image_search"], true);
    let keys: std::collections::HashSet<_> = web0
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        std::collections::HashSet::from(["type", "filters", "enable_image_search"])
    );

    let web1 = &tools[1];
    assert_eq!(web1["type"], "web_search");
    assert!(web1.get("excluded_domains").is_none());
    assert_eq!(
        web1["filters"]["excluded_domains"],
        json!(["spam.test", "ads.test"])
    );
    assert_eq!(web1["enable_image_understanding"], false);

    let web2 = &tools[2];
    assert_eq!(web2["filters"]["allowed_domains"], json!(["keep.me"]));
    assert!(web2["filters"].get("excluded_domains").is_none());

    // function tools are left alone (including unknown fields)
    assert_eq!(tools[3]["name"], "echo");
    assert_eq!(tools[3]["external_web_access"], false);

    // bare x_search is injected when missing
    assert_eq!(tools[4], json!({"type": "x_search"}));
}

#[test]
fn grok_maps_openai_search_content_types_image_to_enable_image_search() {
    let body = json!({
        "tools": [
            {
                "type": "web_search",
                "external_web_access": false,
                "search_content_types": ["text", "image"]
            },
            {
                // explicit flag wins over content-types mapping
                "type": "web_search",
                "search_content_types": ["image"],
                "enable_image_search": false
            },
            {
                "type": "web_search",
                "search_content_types": ["text"]
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    // 3 web_search + injected bare x_search
    assert_eq!(tools.len(), 4);

    assert_eq!(tools[0]["type"], "web_search");
    assert_eq!(tools[0]["enable_image_search"], true);
    assert!(tools[0].get("search_content_types").is_none());
    assert!(tools[0].get("external_web_access").is_none());

    assert_eq!(tools[1]["enable_image_search"], false);
    assert!(tools[1].get("search_content_types").is_none());

    assert!(tools[2].get("enable_image_search").is_none());
    assert!(tools[2].get("search_content_types").is_none());

    assert_eq!(tools[3], json!({"type": "x_search"}));
}


#[test]
fn grok_profile_injects_x_search_when_tools_present() {
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {"type": "function", "name": "echo", "parameters": {"type": "object"}}
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "echo");
    assert_eq!(tools[1], json!({"type": "x_search"}));
}

#[test]
fn grok_profile_does_not_create_tools_only_for_x_search() {
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    assert!(out.get("tools").is_none());
}

#[test]
fn grok_profile_does_not_duplicate_existing_x_search() {
    let body = json!({
        "tools": [
            {
                "type": "x_search",
                "allowed_x_handles": ["elonmusk"]
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "x_search");
    assert_eq!(tools[0]["allowed_x_handles"], json!(["elonmusk"]));
}

#[test]
fn grok_profile_adapts_x_search_to_xai_responses_shape() {
    // Matches https://docs.x.ai/developers/tools/x-search Responses examples.
    let body = json!({
        "model": "grok-4.5",
        "input": [{"role": "user", "content": "hi"}],
        "tools": [
            {
                "type": "x_search",
                // camelCase AI SDK aliases
                "allowedXHandles": [
                    "a","b","c","d","e","f","g","h","i","j",
                    "k","l","m","n","o","p","q","r","s","t","u"
                ],
                "excludedXHandles": ["spam"],
                "fromDate": "2025-10-01",
                "toDate": "2025-10-10",
                "enableImageUnderstanding": true,
                "enableVideoUnderstanding": false,
                "unknown_field": true
            },
            {
                // both lists present with snake_case: keep allowed only
                "type": "x_search",
                "allowed_x_handles": ["keep_me"],
                "excluded_x_handles": ["drop_me"],
                "from_date": "2025-01-01"
            },
            {
                // canonical key wins over alias
                "type": "x_search",
                "allowed_x_handles": ["canonical"],
                "allowedXHandles": ["alias"]
            }
        ]
    });

    let out = rewrite_request(ProviderType::Grok, body).unwrap();
    let tools = out.get("tools").and_then(Value::as_array).unwrap();
    assert_eq!(tools.len(), 3);

    let x0 = &tools[0];
    assert_eq!(x0["type"], "x_search");
    assert!(x0.get("allowedXHandles").is_none());
    assert!(x0.get("excludedXHandles").is_none());
    assert!(x0.get("fromDate").is_none());
    assert!(x0.get("toDate").is_none());
    assert!(x0.get("enableImageUnderstanding").is_none());
    assert!(x0.get("enableVideoUnderstanding").is_none());
    assert!(x0.get("unknown_field").is_none());
    // max 20 handles; prefer allowed over excluded
    assert_eq!(
        x0["allowed_x_handles"].as_array().unwrap().len(),
        20
    );
    assert!(x0.get("excluded_x_handles").is_none());
    assert_eq!(x0["from_date"], "2025-10-01");
    assert_eq!(x0["to_date"], "2025-10-10");
    assert_eq!(x0["enable_image_understanding"], true);
    assert_eq!(x0["enable_video_understanding"], false);

    let x1 = &tools[1];
    assert_eq!(x1["allowed_x_handles"], json!(["keep_me"]));
    assert!(x1.get("excluded_x_handles").is_none());
    assert_eq!(x1["from_date"], "2025-01-01");

    let x2 = &tools[2];
    assert_eq!(x2["allowed_x_handles"], json!(["canonical"]));
    assert!(x2.get("allowedXHandles").is_none());
}

#[test]
fn passthrough_rewrite_is_identity() {
    let body = json!({"model": "x", "tools": [{"type": "function", "name": "a"}]});
    let out = rewrite_request(ProviderType::Responses, body.clone()).unwrap();
    assert_eq!(out, body);
}
