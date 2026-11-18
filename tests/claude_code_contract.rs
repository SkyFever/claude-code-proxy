//! Regression cases from the 2026-11-19 compatibility review.
//! Exercise Anthropic request contracts with synthetic data and no live API.

use claude_code_proxy::anthropic::schema::MessagesRequest;
use claude_code_proxy::providers::codex::translate::request::{
    ResponsesInputItem, ResponsesRequest, ResponsesTool, TranslateOptions, translate_request,
};
use serde_json::{Value, json};

fn convert(value: Value) -> anyhow::Result<ResponsesRequest> {
    convert_lane(value, false)
}

fn convert_lane(value: Value, use_responses_lite: bool) -> anyhow::Result<ResponsesRequest> {
    let req: MessagesRequest = serde_json::from_value(value)?;
    translate_request(
        &req,
        TranslateOptions {
            session_id: None,
            service_tier: None,
            model: "gpt-5.5".into(),
            use_responses_lite,
        },
    )
}

fn basic() -> Value {
    json!({"model":"gpt-5.5","max_tokens":64,"messages":[{"role":"user","content":"hello"}]})
}

fn search_request() -> Value {
    json!({
        "model":"gpt-5.5","max_tokens":64,
        "tools":[
            {"name":"ToolSearch","input_schema":{"type":"object","properties":{"query":{"type":"string"}}}},
            {"type":"custom","name":"mcp__files__read","defer_loading":true,"input_schema":{"type":"object","properties":{}}}
        ],
        "messages":[
            {"role":"user","content":"Find a file tool"},
            {"role":"assistant","content":[{"type":"tool_use","id":"toolu_search","name":"ToolSearch","input":{"query":"files"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_search","content":[{"type":"tool_reference","tool_name":"mcp__files__read"}]}]}
        ]
    })
}

fn registered_function(result: &ResponsesRequest, name: &str, lite: bool) -> bool {
    if lite {
        result.input.iter().any(|item| {
            matches!(item, ResponsesInputItem::AdditionalTools { tools, .. }
                if tools.iter().any(|tool| tool["type"] == "function" && tool["name"] == name))
        })
    } else {
        result.tools.as_ref().is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| matches!(tool, ResponsesTool::Function(tool) if tool.name == name))
        })
    }
}

fn search_output(result: &ResponsesRequest) -> Value {
    let item = result
        .input
        .iter()
        .find(|item| {
            matches!(item, ResponsesInputItem::FunctionCallOutput { call_id, .. }
            if call_id == "toolu_search")
        })
        .expect("The search call must have a result with the same ID");
    serde_json::to_value(item).unwrap()["output"].clone()
}

#[test]
fn control_plain_text_request_is_accepted() {
    assert!(convert(basic()).is_ok());
}

#[test]
fn ordinary_tool_type_spellings_are_accepted_in_both_lanes() {
    for lite in [false, true] {
        for kind in [None, Some("function")] {
            let mut req = basic();
            let mut tool = json!({"name":"Bash","input_schema":{"type":"object","properties":{}}});
            if let Some(kind) = kind {
                tool["type"] = json!(kind);
            }
            req["tools"] = json!([tool]);
            let result = convert_lane(req, lite).unwrap();
            assert!(registered_function(&result, "Bash", lite));
        }
    }
}

#[test]
fn valid_anthropic_custom_tool_type_is_accepted() {
    for lite in [false, true] {
        let mut req = basic();
        req["tools"] = json!([{"type":"custom","name":"Bash","strict":false,"input_schema":{"type":"object","properties":{}}}]);
        let result =
            convert_lane(req, lite).expect("Anthropic ToolParam allows an explicit custom type");
        assert!(registered_function(&result, "Bash", lite));
    }
}

#[test]
fn valid_nullable_output_format_is_accepted() {
    for lite in [false, true] {
        let mut req = basic();
        req["output_config"] = json!({"format":null,"effort":"low"});
        let result = convert_lane(req, lite)
            .expect("A null output_config.format means no structured output format");
        let wire = serde_json::to_value(result).unwrap();
        assert!(wire.pointer("/text/format").is_none_or(Value::is_null));
        assert_eq!(wire.pointer("/reasoning/effort"), Some(&json!("low")));
    }
}

#[test]
fn discovered_tool_name_survives_nested_tool_result() {
    for lite in [false, true] {
        let result = convert_lane(search_request(), lite).unwrap();
        assert!(registered_function(&result, "mcp__files__read", lite));
        assert!(result.input.iter().any(|item| {
            matches!(item, ResponsesInputItem::FunctionCall { call_id, name, arguments }
                if call_id == "toolu_search" && name == "ToolSearch"
                    && serde_json::from_str::<Value>(arguments).unwrap() == json!({"query":"files"}))
        }));
        let output = search_output(&result);
        assert!(
            output.as_str().unwrap().contains("mcp__files__read"),
            "Search-result data was lost: {output}"
        );
        assert!(
            !output
                .as_str()
                .unwrap()
                .contains("unsupported content block omitted")
        );
    }
}

#[test]
fn unsupported_advisor_error_uses_documented_recovery_marker() {
    for lite in [false, true] {
        let mut req = basic();
        req["tools"] = json!([{"type":"advisor_20260301","name":"advisor"}]);
        let error = convert_lane(req, lite).unwrap_err().to_string();
        assert!(
            error.contains("Input tag 'advisor_20260301'"),
            "Claude Code documents this marker for its advisor fallback; actual: {error}"
        );
    }
}

#[test]
fn strict_mode_is_explicitly_unsupported_for_custom_tools_too() {
    for lite in [false, true] {
        for kind in ["function", "custom"] {
            let mut req = basic();
            req["tools"] = json!([{"type":kind,"name":"Bash","strict":true,"input_schema":{"type":"object","properties":{}}}]);
            let error = convert_lane(req, lite).unwrap_err().to_string();
            assert!(error.contains("strict schema enforcement"));
        }
    }
}

#[test]
fn nested_tool_references_require_nonempty_string_names() {
    for lite in [false, true] {
        for invalid in [Value::Null, json!(42), json!(""), json!(" \t")] {
            let mut req = search_request();
            req["messages"][2]["content"][0]["content"][0]["tool_name"] = invalid;
            let error = convert_lane(req, lite).unwrap_err().to_string();
            assert!(error.contains("tool_name must be a nonempty string"));
            assert!(error.contains("messages[2].content[0].content[0]"));
        }
        let mut req = search_request();
        req["messages"][2]["content"][0]["content"][0]
            .as_object_mut()
            .unwrap()
            .remove("tool_name");
        assert!(convert_lane(req, lite).is_err());
    }
}

#[test]
fn nested_tool_references_must_match_a_current_tool_definition() {
    for lite in [false, true] {
        for name in ["mcp__files__missing", "MCP__files__read"] {
            let mut req = search_request();
            req["messages"][2]["content"][0]["content"][0]["tool_name"] = json!(name);
            let error = convert_lane(req, lite).unwrap_err().to_string();
            assert!(error.contains(name));
            assert!(error.contains("no matching top-level tool definition"));
        }
    }
}

#[test]
fn a_single_reference_object_is_not_silently_dropped() {
    for lite in [false, true] {
        let mut req = search_request();
        req["messages"][2]["content"][0]["content"] =
            json!({"type":"tool_reference","tool_name":"mcp__files__read"});
        let error = convert_lane(req, lite).unwrap_err().to_string();
        assert!(error.contains("tool result blocks must be an array"));
    }
}

#[test]
fn mixed_reference_results_preserve_order_images_error_and_call_id() {
    const PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
    for lite in [false, true] {
        let mut req = search_request();
        req["tools"].as_array_mut().unwrap().push(json!({
            "name":"mcp__files__write","defer_loading":true,"input_schema":{"type":"object"}
        }));
        req["messages"][2]["content"] = json!([
            {"type":"text","text":"before result"},
            {"type":"tool_result","tool_use_id":"toolu_search","is_error":true,"content":[
                {"type":"text","text":"prefix"},
                {"type":"tool_reference","tool_name":"mcp__files__read"},
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":PNG_BASE64}},
                {"type":"tool_reference","tool_name":"mcp__files__write"},
                {"type":"text","text":"suffix"}
            ]},
            {"type":"text","text":"after result"}
        ]);
        let result = convert_lane(req, lite).unwrap();
        let output = search_output(&result);
        let parts = output
            .as_array()
            .expect("Image-bearing tool output retains content blocks");
        assert_eq!(parts.len(), 6);
        assert_eq!(parts[0]["text"], "[tool execution error]");
        assert_eq!(parts[1]["text"], "prefix");
        assert!(
            parts[2]["text"]
                .as_str()
                .unwrap()
                .contains("mcp__files__read")
        );
        assert_eq!(parts[3]["type"], "input_image");
        assert_eq!(
            parts[3]["image_url"],
            format!("data:image/png;base64,{PNG_BASE64}")
        );
        assert!(
            parts[4]["text"]
                .as_str()
                .unwrap()
                .contains("mcp__files__write")
        );
        assert_eq!(parts[5]["text"], "suffix");
        let index = result.input.iter().position(|item| {
            matches!(item, ResponsesInputItem::FunctionCallOutput { call_id, .. } if call_id == "toolu_search")
        }).unwrap();
        let before = serde_json::to_value(&result.input[index - 1]).unwrap();
        let after = serde_json::to_value(&result.input[index + 1]).unwrap();
        assert_eq!(before["content"][0]["text"], "before result");
        assert_eq!(after["content"][0]["text"], "after result");
    }
}
