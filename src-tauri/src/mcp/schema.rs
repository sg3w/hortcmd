//! Validate the deliberately small JSON Schema vocabulary used by our tools.
//! Unknown schema types fail closed rather than silently accepting arguments.
use serde_json::Value;

pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            let object = value.as_object().ok_or("arguments must be an object")?;
            let properties = schema["properties"]
                .as_object()
                .ok_or("invalid object schema")?;
            if let Some(required) = schema["required"].as_array() {
                for key in required.iter().filter_map(Value::as_str) {
                    if !object.contains_key(key) {
                        return Err(format!("missing argument: {key}"));
                    }
                }
            }
            for (key, value) in object {
                let field = properties
                    .get(key)
                    .ok_or_else(|| format!("unknown argument: {key}"))?;
                validate(field, value).map_err(|e| format!("{key}: {e}"))?;
            }
        }
        Some("string") => {
            let text = value.as_str().ok_or("must be a string")?;
            if schema["minLength"]
                .as_u64()
                .is_some_and(|min| (text.chars().count() as u64) < min)
            {
                return Err("string is too short".into());
            }
        }
        Some("boolean") if value.is_boolean() => {}
        Some("integer") => {
            let n = value.as_u64().ok_or("must be a nonnegative integer")?;
            if schema["minimum"].as_u64().is_some_and(|min| n < min)
                || schema["maximum"].as_u64().is_some_and(|max| n > max)
            {
                return Err("integer is outside the allowed range".into());
            }
        }
        Some("array") => {
            let items = value.as_array().ok_or("must be an array")?;
            if schema["minItems"]
                .as_u64()
                .is_some_and(|min| (items.len() as u64) < min)
            {
                return Err("too few array items".into());
            }
            for item in items {
                validate(&schema["items"], item)?;
            }
        }
        _ => return Err("argument type does not match the schema".into()),
    }
    if schema["enum"]
        .as_array()
        .is_some_and(|allowed| !allowed.contains(value))
    {
        return Err("value is not in the allowed enumeration".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn rejects_malformed_arguments_against_actual_tool_schemas() {
        for (tool, args) in [
            (
                "write_file",
                json!({"path":"/x","contents":"x","overwrite":"true"}),
            ),
            (
                "write_file",
                json!({"path":"/x","contents":"x","unexpected":true}),
            ),
            ("copy", json!({"from":"/x"})),
            ("search", json!({"root":"/x","min_size":-1})),
            ("search", json!({"root":"/x","limit":1.5})),
            ("search", json!({"root":"/x","mode":"unknown"})),
            ("create_archive", json!({"sources":["/x",42],"to":"/y"})),
            ("create_archive", json!({"sources":[],"to":"/y"})),
            ("list_dir", json!({"path":""})),
        ] {
            assert!(
                validate(&(super::super::tools::find(tool).unwrap().schema)(), &args).is_err(),
                "{tool}: {args}"
            );
        }
    }
}
