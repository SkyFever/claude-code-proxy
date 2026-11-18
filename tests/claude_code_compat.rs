//! Regression tests for the Claude Code -> Anthropic Messages -> Codex bridge.
//! The tests do not require credentials or live network calls.

use claude_code_proxy::anthropic::schema::MessagesRequest;
use claude_code_proxy::providers::codex::translate::request::{
    ResponsesInputItem, ResponsesTool, TranslateOptions, translate_request,
};
use serde_json::json;

fn convert(
    value: serde_json::Value,
) -> Result<claude_code_proxy::providers::codex::translate::request::ResponsesRequest, anyhow::Error>
{
    let req: MessagesRequest = serde_json::from_value(value)?;
    translate_request(
        &req,
        TranslateOptions {
            session_id: None,
            service_tier: None,
            model: "gpt-5.5".into(),
            use_responses_lite: false,
        },
    )
}

fn basic() -> serde_json::Value {
    json!({
        "model": "gpt-5.5",
        "messages": [{"role": "user", "content": "hello"}]
    })
}

#[test]
fn tool_search_fallback_eagerly_registers_all_deferred_functions() {
    let mut request = basic();
    request["tools"] = json!([
        {"type": "tool_search_tool_bm25_20251119", "name": "tool_search"},
        {"name": "mcp__files__read", "defer_loading": true,
         "description": "Read file", "input_schema": {"type": "object"}},
        {"name": "Bash", "input_schema": {"type": "object"}}
    ]);
    let out = convert(request).unwrap();
    let tools = out.tools.unwrap();
    assert_eq!(tools.len(), 2);
    assert!(matches!(&tools[0], ResponsesTool::Function(tool) if tool.name == "mcp__files__read"));
    assert!(matches!(&tools[1], ResponsesTool::Function(tool) if tool.name == "Bash"));
}

#[test]
fn generated_server_tool_history_is_preserved_as_readable_text() {
    let request = json!({
        "model": "gpt-5.5",
        "messages": [
            {"role":"user", "content":"Find the manual"},
            {"role":"assistant", "content": [
                {"type":"server_tool_use", "id":"srvtoolu_1", "name":"web_search",
                 "input":{"query":"manual"}},
                {"type":"web_search_tool_result", "tool_use_id":"srvtoolu_1",
                 "content":[{"type":"web_search_result", "title":"Manual"}]},
                {"type":"tool_search_tool_result", "content":[
                    {"type":"tool_reference", "tool_name":"mcp__files__read"}
                ]},
                {"type":"tool_reference", "tool_name":"mcp__files__read"}
            ]},
            {"role":"user", "content":"Continue"}
        ]
    });
    let result = convert(request).unwrap();
    let combined = serde_json::to_string(&result.input).unwrap();
    assert!(combined.contains("Historical Anthropic server tool call"));
    assert!(combined.contains("Manual"));
    assert!(combined.contains("mcp__files__read"));
}

#[test]
fn unknown_content_block_is_not_silently_removed() {
    let request = json!({
        "model": "gpt-5.5",
        "messages": [{"role":"user", "content": [
            {"type":"text", "text":"hello"},
            {"type":"document", "source":{"type":"base64", "data":"abc"}}
        ]}]
    });
    let error = convert(request).err().unwrap().to_string();
    assert!(error.contains("document"));
    assert!(error.contains("messages[0].content[1]"));
}

#[test]
fn malformed_tools_and_unsupported_tool_types_do_not_become_fake_functions() {
    for tool in [
        json!({"type":"mcp_toolset", "name":"example"}),
        json!({"type":"function", "name":"Bash", "input_schema":{}, "strict":true}),
        json!({"type":"function", "name":"Bash", "input_schema":{},
               "allowed_callers":["code_execution_20260120"]}),
        json!({"type":0, "name":"Bash"}),
    ] {
        let mut req = basic();
        req["tools"] = json!([tool]);
        assert!(convert(req).is_err());
    }
}

#[test]
fn unknown_tool_choice_and_output_format_are_errors() {
    let mut req = basic();
    req["tool_choice"] = json!({"type":"unknown"});
    assert!(
        convert(req)
            .err()
            .unwrap()
            .to_string()
            .contains("tool_choice")
    );

    let mut req = basic();
    req["output_config"] = json!({"format":{"type":"xml"}});
    assert!(convert(req).err().unwrap().to_string().contains("xml"));
}

#[test]
fn a_forced_tool_search_is_not_accepted_as_a_search_result() {
    let mut req = basic();
    req["tools"] = json!([
        {"type":"tool_search_tool_regex_20251119", "name":"tool_search"}
    ]);
    req["tool_choice"] = json!({"type":"tool", "name":"tool_search"});
    assert!(
        convert(req)
            .err()
            .unwrap()
            .to_string()
            .contains("cannot force a search")
    );
}

#[test]
fn malformed_tool_use_and_non_anthropic_roles_are_rejected() {
    let req = json!({
        "model":"gpt-5.5",
        "messages":[{"role":"assistant", "content":[{"type":"tool_use","name":"Bash"}]}]
    });
    assert!(
        convert(req)
            .err()
            .unwrap()
            .to_string()
            .contains("Malformed tool_use")
    );

    let req = json!({"model":"gpt-5.5", "messages":[{"role":"developer","content":"hi"}]});
    assert!(
        convert(req)
            .err()
            .unwrap()
            .to_string()
            .contains("Unsupported role")
    );
}

#[test]
fn ordinary_tool_use_roundtrip_retains_call_id_and_arguments() {
    let req = json!({
        "model":"gpt-5.5",
        "messages": [
            {"role":"user", "content":"read foo"},
            {"role":"assistant", "content":[
                {"type":"tool_use", "id":"toolu_123", "name":"Read", "input":{"file_path":"/tmp/foo"}}
            ]},
            {"role":"user", "content":[
                {"type":"tool_result", "tool_use_id":"toolu_123", "content":"hello"}
            ]}
        ]
    });
    let out = convert(req).unwrap();
    assert!(out.input.iter().any(|item| matches!(item,
        ResponsesInputItem::FunctionCall { call_id, name, .. }
        if call_id == "toolu_123" && name == "Read"
    )));
    assert!(out.input.iter().any(|item| matches!(item,
        ResponsesInputItem::FunctionCallOutput { call_id, .. }
        if call_id == "toolu_123"
    )));
}
