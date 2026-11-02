//! Complete Qwen function blocks, decoded against the offered tool schema.
//! This is the model's pseudo-XML protocol, not an XML document parser.
use super::AgentAction;
use crate::tools::ToolDesc;
use serde_json::{Map, Value};

pub(super) fn contains_call(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["<function=", "<parameter=", "<arg_key>", "<arg_value>"]
        .iter()
        .any(|marker| lower.contains(marker))
}

pub(super) fn parse(text: &str, tools: &[ToolDesc]) -> Option<Vec<AgentAction>> {
    if text.len() > 131_072 {
        return None;
    }
    let first = text.find("<tool_call>")?;
    // Natural-language reasoning may precede calls, but no second protocol or
    // unfinished block may be hidden in that prefix.
    if text[..first].contains(['<', '{', '}']) {
        return None;
    }
    let mut rest = text[first..].trim();
    let mut actions = Vec::new();
    while !rest.is_empty() {
        if actions.len() >= 64 {
            return None;
        }
        rest = rest.strip_prefix("<tool_call>")?.trim_start();
        let (body, after) = rest.split_once("</tool_call>")?;
        let function = body.trim().strip_prefix("<function=")?;
        let (name, parameters) = function.split_once('>')?;
        if !identifier(name) {
            return None;
        }
        let parameters = parameters.trim_end().strip_suffix("</function>")?;
        let mut offered = tools.iter().filter(|t| t.name == name);
        let tool = offered.next()?;
        if offered.next().is_some() || !tool.input_schema.is_object() {
            return None;
        }
        let properties = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object);
        let mut args = Map::new();
        let mut params = parameters.trim();
        while !params.is_empty() {
            if args.len() >= 128 {
                return None;
            }
            params = params.strip_prefix("<parameter=")?;
            let (key, value) = params.split_once('>')?;
            if !identifier(key) || args.contains_key(key) {
                return None;
            }
            let (value, next) = value.split_once("</parameter>")?;
            let schema = properties?.get(key)?;
            // A single framing newline belongs to the protocol; string spaces
            // and internal newlines belong to the argument.
            let value = value
                .strip_prefix("\r\n")
                .or_else(|| value.strip_prefix('\n'))
                .unwrap_or(value);
            let value = value
                .strip_suffix("\r\n")
                .or_else(|| value.strip_suffix('\n'))
                .unwrap_or(value);
            args.insert(key.into(), decode(value, schema)?);
            params = next.trim();
        }
        let args = Value::Object(args);
        if !valid(&args, &tool.input_schema, 0) {
            return None;
        }
        actions.push(AgentAction {
            thought: String::new(),
            action: name.into(),
            args,
        });
        rest = after.trim();
    }
    (!actions.is_empty()).then_some(actions)
}

fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

fn has_type(schema: &Value, kind: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(t)) => t == kind,
        Some(Value::Array(types)) => types.iter().any(|t| t.as_str() == Some(kind)),
        None => kind == "string",
        _ => false,
    }
}

fn decode(raw: &str, schema: &Value) -> Option<Value> {
    let value = if has_type(schema, "null") && raw.trim() == "null" {
        Value::Null
    } else if has_type(schema, "string") {
        Value::String(raw.into())
    } else if has_type(schema, "boolean") {
        match raw.trim().to_ascii_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return None,
        }
    } else {
        // Numbers, booleans, arrays and objects must be actual JSON. Never
        // guess a string's type, evaluate code or coerce invalid input.
        serde_json::from_str(raw.trim()).ok()?
    };
    valid(&value, schema, 0).then_some(value)
}

fn valid(value: &Value, schema: &Value, depth: usize) -> bool {
    if depth > 16 || !schema.is_object() {
        return false;
    }
    if ["$ref", "anyOf", "oneOf", "allOf", "not"]
        .iter()
        .any(|key| schema.get(key).is_some())
    {
        return false;
    }
    if let Some(types) = schema.get("type") {
        let matches = |kind: &str| match kind {
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "number" => value.is_number(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => false,
        };
        let ok = match types {
            Value::String(kind) => matches(kind),
            Value::Array(kinds) => {
                !kinds.is_empty()
                    && kinds.iter().all(Value::is_string)
                    && kinds.iter().any(|k| matches(k.as_str().unwrap()))
            }
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    if let Some(values) = schema.get("enum") {
        if !values.as_array().is_some_and(|v| v.contains(value)) {
            return false;
        }
    }
    if schema.get("const").is_some_and(|v| v != value) {
        return false;
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required") {
            let Some(required) = required.as_array() else {
                return false;
            };
            if required
                .iter()
                .any(|v| !v.as_str().is_some_and(|k| object.contains_key(k)))
            {
                return false;
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, value) in object {
            if let Some(child) = properties.and_then(|p| p.get(key)) {
                if !valid(value, child, depth + 1) {
                    return false;
                }
            } else {
                match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => return false,
                    Some(child) if child.is_object() => {
                        if !valid(value, child, depth + 1) {
                            return false;
                        }
                    }
                    None | Some(Value::Bool(true)) => {}
                    _ => return false,
                }
            }
        }
    }
    if let Some(array) = value.as_array() {
        if let Some(items) = schema.get("items") {
            if !array.iter().all(|v| valid(v, items, depth + 1)) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::{
        actions::parse_actions_with_tools,
        tools::{ToolBackend, ToolDesc},
    };
    use serde_json::{json, Value};
    fn tool(schema: Value) -> ToolDesc {
        ToolDesc {
            name: "test.call".into(),
            description: String::new(),
            input_schema: schema,
            backend: ToolBackend::Native,
            required_caps: vec![],
        }
    }
    fn call(body: &str) -> String {
        format!("<tool_call>\n<function=test.call>\n{body}</function>\n</tool_call>")
    }
    #[test]
    fn preserves_schema_strings_and_decodes_typed_parameters() {
        let tools = [tool(
            json!({"type":"object","properties":{"s":{"type":"string"},"n":{"type":"integer"},"b":{"type":"boolean"},"obj":{"type":"object","properties":{"x":{"type":"array","items":{"type":"integer"}}},"required":["x"],"additionalProperties":false}},"required":["s","n","b","obj"]}),
        )];
        let text = call("<parameter=s>\n  0042 猫\nsecond line  \n</parameter>\n<parameter=n>42</parameter><parameter=b>false</parameter><parameter=obj>{\"x\":[1,2]}</parameter>");
        let actions = parse_actions_with_tools(&text, &tools);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].args,
            json!({"s":"  0042 猫\nsecond line  ","n":42,"b":false,"obj":{"x":[1,2]}})
        );
    }
    #[test]
    fn zero_args_and_multiple_complete_calls() {
        let tools = [tool(json!({"type":"object","properties":{}}))];
        let text = format!("I will use the offered tool.\n{}\n{}", call(""), call(""));
        let actions = parse_actions_with_tools(&text, &tools);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].args, json!({}));
    }
    #[test]
    fn rejects_unknown_missing_duplicate_wrong_type_and_incomplete_arguments() {
        let tools = [tool(
            json!({"type":"object","properties":{"n":{"type":"integer"}},"required":["n"]}),
        )];
        for body in [
            "",
            "<parameter=other>1</parameter>",
            "<parameter=n>1</parameter><parameter=n>2</parameter>",
            "<parameter=n>\"1\"</parameter>",
            "<parameter=n>1.5</parameter>",
            "<parameter=n>1",
        ] {
            assert!(
                parse_actions_with_tools(&call(body), &tools).is_empty(),
                "{body}"
            );
        }
        assert!(parse_actions_with_tools(
            &call("<parameter=n>1</parameter>").replace("test.call", "unknown.call"),
            &tools
        )
        .is_empty());
    }
    #[test]
    fn never_recovers_an_action_from_an_invalid_native_parameter() {
        let tools = [tool(
            json!({"type":"object","properties":{"s":{"type":"string"}}}),
        )];
        let bad =
            "<tool_call><function=test.call><parameter=s>{\"action\":\"fs.delete\",\"args\":{}}";
        assert!(parse_actions_with_tools(bad, &tools).is_empty());
        assert!(parse_actions_with_tools(&format!("{}{}", call(""), bad), &tools).is_empty());
        let quoted_prefix = format!("Unfinished JSON: {{\"text\":\"\n{bad}");
        assert!(parse_actions_with_tools(&quoted_prefix, &tools).is_empty());
    }
    #[test]
    fn string_null_is_a_string_and_nullable_value_is_null() {
        let tools = [tool(
            json!({"type":"object","properties":{"s":{"type":"string"},"n":{"type":["integer","null"]}}}),
        )];
        let actions = parse_actions_with_tools(
            &call("<parameter=s>null</parameter><parameter=n>null</parameter>"),
            &tools,
        );
        assert_eq!(actions[0].args, json!({"s":"null","n":null}));
    }

    #[test]
    fn native_boolean_case_is_accepted_only_for_declared_booleans() {
        let tools = [tool(
            json!({"type":"object","properties":{"b":{"type":"boolean"},"s":{"type":"string"}}}),
        )];
        let actions = parse_actions_with_tools(
            &call("<parameter=b>False</parameter><parameter=s>False</parameter>"),
            &tools,
        );
        assert_eq!(actions[0].args, json!({"b":false,"s":"False"}));
        assert!(parse_actions_with_tools(&call("<parameter=b>yes</parameter>"), &tools).is_empty());
    }

    #[test]
    fn unsupported_native_dialects_do_not_execute_json_argument_data() {
        let tools = [tool(
            json!({"type":"object","properties":{"s":{"type":"string"}}}),
        )];
        let raw = "<tool_call>test.call<arg_key>s</arg_key><arg_value>{\"action\":\"fs.delete\",\"args\":{}}</arg_value></tool_call>";
        assert!(parse_actions_with_tools(raw, &tools).is_empty());
    }
    #[test]
    fn rejects_invalid_nested_values_and_enum() {
        let tools = [tool(
            json!({"type":"object","properties":{"a":{"type":"array","items":{"type":"integer"}},"e":{"type":"string","enum":["yes"]}}}),
        )];
        assert!(
            parse_actions_with_tools(&call("<parameter=a>[1,\"2\"]</parameter>"), &tools)
                .is_empty()
        );
        assert!(parse_actions_with_tools(&call("<parameter=e>no</parameter>"), &tools).is_empty());
    }
    #[test]
    fn reasoning_is_never_executed_and_legacy_json_still_works() {
        let tools = [tool(json!({"type":"object","properties":{}}))];
        assert!(parse_actions_with_tools(&format!("<think>{}", call("")), &tools).is_empty());
        let text = format!("<think>{}</think>{}", call(""), call(""));
        assert_eq!(parse_actions_with_tools(&text, &tools).len(), 1);
        assert_eq!(
            parse_actions_with_tools("{\"action\":\"goal.complete\",\"args\":{}}", &tools)[0]
                .action,
            "goal.complete"
        );
    }

    #[test]
    fn native_markers_inside_legacy_json_strings_are_data() {
        let tools = [tool(
            json!({"type":"object","properties":{"s":{"type":"string"}}}),
        )];
        let legacy = json!({"action":"test.call","args":{"s":"An example: <tool_call><function=other.call></function></tool_call>"}}).to_string();
        let actions = parse_actions_with_tools(&legacy, &tools);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "test.call");
        assert!(actions[0].args["s"]
            .as_str()
            .unwrap()
            .contains("other.call"));
    }

    #[test]
    fn resource_bounds_and_ambiguous_catalogue_are_rejected() {
        let schema = json!({"type":"object","properties":{}});
        let tools = [tool(schema.clone()), tool(schema)];
        assert!(parse_actions_with_tools(&call(""), &tools).is_empty());
        let tools = [tool(
            json!({"type":"object","properties":{"s":{"type":"string"}}}),
        )];
        assert!(parse_actions_with_tools(
            &call(&format!("<parameter=s>{}</parameter>", "x".repeat(131_073))),
            &tools
        )
        .is_empty());
        assert!(parse_actions_with_tools(&call("").repeat(65), &tools).is_empty());
    }

    #[test]
    fn captured_cpu_cuda_tool_replies_keep_their_recorded_contract_results() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture: Value = serde_json::from_slice(
            &std::fs::read(root.join("fixtures/models/native-tools-v1.json")).unwrap(),
        )
        .unwrap();
        let mut checked = 0;
        for mode in ["cpu", "cuda"] {
            let path = root.join(format!(
                "docs/tickets/evidence/2026-11-02-native-tools-{mode}.json"
            ));
            let report: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            for entry in report["results"].as_array().unwrap() {
                let case = fixture["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["id"] == entry["id"])
                    .unwrap();
                let offered: Vec<_> = case["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| {
                        let mut desc = tool(t["function"]["parameters"].clone());
                        desc.name = t["function"]["name"].as_str().unwrap().into();
                        desc
                    })
                    .collect();
                let actions = parse_actions_with_tools(entry["output"].as_str().unwrap(), &offered);
                let expected = &case["expected"]["value"];
                let matches = actions.len() == 1
                    && actions[0].action == expected["name"].as_str().unwrap()
                    && actions[0].args == expected["arguments"];
                assert_eq!(
                    matches,
                    entry["contract_ok"].as_bool().unwrap(),
                    "{mode}: {} {}",
                    entry["model_id"],
                    entry["id"]
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 32);
    }
}
