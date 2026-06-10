use crate::types::*;
use serde_json::{json, Map, Value};

pub(crate) fn resource_read_result(uri: String, mime_type: &'static str, text: String) -> Value {
    json!(ResourceReadResult {
        contents: vec![ResourceContent {
            mime_type,
            text,
            uri,
        }],
    })
}

pub(crate) struct ResourceDef {
    pub(crate) uri: &'static str,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) mime_type: &'static str,
    pub(crate) files: &'static [&'static str],
}

pub(crate) fn resource_defs() -> &'static [ResourceDef] {
    &[
        ResourceDef {
            uri: "ctx://docs/readme",
            name: "ctx README",
            description: "Top-level project README — usage, features, and command reference.",
            mime_type: "text/markdown",
            files: &["README.md", "readme.md"],
        },
        ResourceDef {
            uri: "ctx://docs/spec",
            name: "ctx specification",
            description: "Design specification and architecture notes for ctx.",
            mime_type: "text/markdown",
            files: &["SPEC.md", "docs/spec.md"],
        },
        ResourceDef {
            uri: "ctx://docs/audit-schema",
            name: "Audit log JSON schema",
            description: "JSON schema describing the structure of ctx audit log entries.",
            mime_type: "application/json",
            files: &["docs/schema.json"],
        },
        ResourceDef {
            uri: "ctx://config/example",
            name: "ctx.toml example",
            description: "Annotated example configuration file showing all supported keys.",
            mime_type: "text/plain",
            files: &["ctx.toml.example"],
        },
    ]
}

pub(crate) fn resource_value(def: &ResourceDef) -> Value {
    let mut m = Map::new();
    m.insert("description".to_string(), json!(def.description));
    m.insert("mimeType".to_string(), json!(def.mime_type));
    m.insert("name".to_string(), json!(def.name));
    m.insert("uri".to_string(), json!(def.uri));
    Value::Object(m)
}

pub(crate) fn resource_templates() -> Vec<Value> {
    let mut m = Map::new();
    m.insert(
        "description".to_string(),
        json!("Read any file inside the configured server root. {path} is repository-relative; symlinks that escape the root are rejected."),
    );
    m.insert("mimeType".to_string(), json!("text/plain"));
    m.insert("name".to_string(), json!("Repository file"));
    m.insert("uriTemplate".to_string(), json!("ctx://file/{path}"));
    vec![Value::Object(m)]
}
