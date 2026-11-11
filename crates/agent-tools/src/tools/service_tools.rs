//! Shared plumbing for the MCP tools that wrap a host service directly
//! (documents, contacts, rates): the `service_tool!` macro, the service
//! lookup, and an audit sanitizer that keeps only opaque ids and flags.

use serde_json::{json, Map, Value};

use crate::tool::{AgentToolError, AgentToolResult};

/// The service a tool needs, or an error saying this environment has none.
pub(crate) fn need<T>(service: Option<T>, what: &str) -> Result<T, AgentToolError> {
    service.ok_or_else(|| {
        AgentToolError::ExecutionFailed(format!("{what} is not available in this environment"))
    })
}

pub(crate) fn fail(error: impl std::fmt::Display) -> AgentToolError {
    AgentToolError::ExecutionFailed(error.to_string())
}

pub(crate) fn reply<T: serde::Serialize>(value: &T) -> Result<AgentToolResult, AgentToolError> {
    Ok(AgentToolResult {
        content: serde_json::to_value(value)?,
    })
}

/// What reaches the audit log: ids (opaque), booleans and the names of the fields given. Names, IBANs, amounts,
/// notes and file contents never do.
pub(crate) fn audit_ids_only(args: &Value) -> Value {
    fn keep(key: &str, value: &Value) -> Value {
        let is_id = key.ends_with("Id") || key.ends_with("Ids") || key == "id";
        match value {
            Value::Bool(_) => value.clone(),
            Value::String(_) | Value::Number(_) if is_id => value.clone(),
            Value::Array(items) if is_id => Value::Array(items.clone()),
            Value::Null => Value::Null,
            Value::Array(items) => json!(format!("[{} items]", items.len())),
            _ => json!("[redacted]"),
        }
    }
    match args {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), keep(key, value)))
                .collect::<Map<_, _>>(),
        ),
        _ => json!("[redacted]"),
    }
}

/// Declares an `AgentTool` that deserializes nothing itself: the body receives the environment and the raw
/// arguments and returns the result.
macro_rules! service_tool {
    (
        $(#[$meta:meta])*
        $ty:ident, $name:literal, $access:ident, additive = $additive:literal,
        scopes = [$($scope:ident),+],
        description = $desc:literal,
        schema = $schema:expr,
        |$env:ident, $args:ident| $body:block
    ) => {
        $(#[$meta])*
        pub struct $ty;

        #[async_trait::async_trait]
        impl crate::tool::AgentTool for $ty {
            fn name(&self) -> &'static str {
                $name
            }
            fn description(&self) -> &'static str {
                $desc
            }
            fn input_schema(&self) -> serde_json::Value {
                $schema
            }
            fn required_scopes(&self) -> &'static [crate::scope::AgentScope] {
                &[$(crate::scope::AgentScope::$scope),+]
            }
            fn access_level(&self) -> crate::tool::AgentToolAccess {
                crate::tool::AgentToolAccess::$access
            }
            fn is_additive(&self) -> bool {
                $additive
            }
            fn sanitize_args_for_audit(&self, args: &serde_json::Value) -> serde_json::Value {
                crate::tools::service_tools::audit_ids_only(args)
            }
            async fn call(
                &self,
                $env: std::sync::Arc<dyn crate::env::AgentEnvironment>,
                $args: serde_json::Value,
            ) -> Result<crate::tool::AgentToolResult, crate::tool::AgentToolError> {
                $body
            }
        }
    };
}
pub(crate) use service_tool;

/// An object schema from its properties and required names.
pub(crate) fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required})
}

/// A schema with no argument.
pub(crate) fn no_args() -> Value {
    object(json!({}), &[])
}
