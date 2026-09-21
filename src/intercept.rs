use base64::{Engine, engine::general_purpose::STANDARD};
use jiff::Timestamp;
use serde_json::{Value, json};

use crate::{
    Runtime,
    error::{Error, Result},
};

pub fn before(runtime: &Runtime, request: &Value, now: Timestamp) -> Result<Value> {
    let scope = request
        .pointer("/Metadata/caller_scope")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if runtime.store.policy_for_scope(scope)?.status(now) != "active" {
        return Ok(reject(Error::new(
            401,
            "invalid_api_key",
            "API key is disabled or expired",
        )));
    }
    // An empty response makes no changes. CPA retains ownership of authentication,
    // headers, body, model selection, streaming and all other request properties.
    Ok(json!({}))
}

pub fn reject(error: Error) -> Value {
    json!({"Terminate": true, "StatusCode": error.status,
        "ResponseHeaders": {"Content-Type": ["application/json"]},
        "ResponseBody": STANDARD.encode(error.json().to_string())})
}
