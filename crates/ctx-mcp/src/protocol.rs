use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, value::RawValue, Value};

#[derive(Deserialize)]
pub(crate) struct Request {
    pub(crate) jsonrpc: String,
    #[serde(default)]
    pub(crate) id: Option<Value>,
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) params: Option<Box<RawValue>>,
}

#[derive(Serialize)]
pub(crate) struct Response {
    pub(crate) jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<RpcError>,
}

#[derive(Serialize)]
pub(crate) struct RpcError {
    pub(crate) code: i32,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct CallParams {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) arguments: Option<Box<RawValue>>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct PageCursor {
    pub(crate) o: usize,
}

pub(crate) fn parse_error_message(line: &str, err: &serde_json::Error) -> String {
    if line.starts_with("{not") {
        return "invalid character 'n' looking for beginning of object key string".to_string();
    }
    err.to_string()
}

pub(crate) fn parse_json<T: for<'de> Deserialize<'de>>(raw: &RawValue) -> Result<T, RpcError> {
    serde_json::from_str(raw.get()).map_err(|err| RpcError {
        code: -32602,
        message: err.to_string(),
        data: None,
    })
}

pub(crate) fn parse_json_opt<T: for<'de> Deserialize<'de>>(
    raw: Option<&RawValue>,
) -> Result<T, RpcError> {
    match raw {
        Some(raw) => parse_json(raw),
        None => serde_json::from_str("{}").map_err(|err| RpcError {
            code: -32602,
            message: err.to_string(),
            data: None,
        }),
    }
}

pub(crate) fn validate_len(field: &str, value: &str, max: usize) -> Result<(), RpcError> {
    if value.len() > max {
        return invalid_params(&format!("{field} exceeds max length {max}"));
    }
    Ok(())
}

pub(crate) fn invalid_params<T>(message: &str) -> Result<T, RpcError> {
    Err(RpcError {
        code: -32602,
        message: message.to_string(),
        data: None,
    })
}

pub(crate) fn invalid_params_with_hint<T>(message: &str, hint: &str) -> Result<T, RpcError> {
    Err(RpcError {
        code: -32602,
        message: message.to_string(),
        data: Some(json!({"hint": hint})),
    })
}

pub(crate) fn tool_error(err: impl ToString) -> RpcError {
    RpcError {
        code: -32000,
        message: err.to_string(),
        data: None,
    }
}

pub(crate) fn tool_error_result(message: &str) -> Value {
    json!({
        "content": [{"type": "text", "text": message}],
        "isError": true,
    })
}

pub(crate) fn resolve_pagination(page_size: i64, cursor: &str) -> Result<(usize, usize), RpcError> {
    let offset = decode_cursor(cursor)?;
    Ok((offset, page_size as usize))
}

pub(crate) fn decode_cursor(cursor: &str) -> Result<usize, RpcError> {
    if cursor.is_empty() {
        return Ok(0);
    }
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| RpcError {
            code: -32602,
            message: "invalid cursor".to_string(),
            data: None,
        })?;
    let parsed: PageCursor = serde_json::from_slice(&raw).map_err(|_| RpcError {
        code: -32602,
        message: "invalid cursor".to_string(),
        data: None,
    })?;
    Ok(parsed.o)
}

pub(crate) fn encode_cursor(offset: usize) -> String {
    if offset == 0 {
        return String::new();
    }
    let raw = serde_json::to_vec(&PageCursor { o: offset }).unwrap_or_default();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

pub(crate) fn apply_page_window<T>(items: &[T], offset: usize, size: usize) -> (&[T], usize) {
    if offset >= items.len() {
        return (&[], 0);
    }
    if size == 0 || offset + size >= items.len() {
        return (&items[offset..], 0);
    }
    (&items[offset..offset + size], offset + size)
}

pub(crate) fn pagination_footer(
    returned: usize,
    total: usize,
    next_offset: usize,
) -> Option<String> {
    if next_offset == 0 {
        None
    } else {
        let cursor = encode_cursor(next_offset);
        Some(format!(
            "\n\nShowing {returned} of {total} (next_cursor='{cursor}')\nCall again with cursor='{cursor}' to get the rest.\n"
        ))
    }
}
