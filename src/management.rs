use base64::{Engine, engine::general_purpose::STANDARD};
use jiff::Timestamp;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use crate::{
    Runtime,
    error::{Error, INVALID_REQUEST, Result},
    store::{KeyEntry, Policy},
};

pub const API_PATH: &str = "/v0/management/key-expiration/keys";
pub const SYNC_PATH: &str = "/v0/management/key-expiration/keys/sync";

pub fn registration() -> Value {
    json!({
        "routes": [
            {"Method": "GET", "Path": API_PATH},
            {"Method": "PUT", "Path": API_PATH},
            {"Method": "POST", "Path": SYNC_PATH}
        ],
        "resources": [
            {"Path": "/index.html", "Menu": "Key 有效期"},
            {"Path": "/app.js"}, {"Path": "/panel.js"}, {"Path": "/calendar.js"}, {"Path": "/style.css"}
        ]
    })
}

pub fn asset(method: &str, path: &str) -> Option<Value> {
    if method != "GET" || !path.starts_with("/v0/resource/plugins/") {
        return None;
    }
    let (mime, bytes) = match path.rsplit('/').next()? {
        "index.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../web/index.html").as_slice(),
        ),
        "app.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/app.js").as_slice(),
        ),
        "panel.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/panel.js").as_slice(),
        ),
        "calendar.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../web/calendar.js").as_slice(),
        ),
        "style.css" => (
            "text/css; charset=utf-8",
            include_bytes!("../web/style.css").as_slice(),
        ),
        _ => return None,
    };
    Some(response(200, mime, bytes))
}

pub fn handle(runtime: &Runtime, method: &str, path: &str, body: &str) -> Result<Value> {
    if path == SYNC_PATH && method == "POST" {
        let sync: Sync = decode(body, 4 * 1024 * 1024)?;
        if !sync.allow_empty && sync.keys.iter().all(|key| key.trim().is_empty()) {
            return Err(INVALID_REQUEST);
        }
        runtime.store.sync(&sync.keys)?;
        return list(runtime);
    }
    if path != API_PATH {
        return Err(Error::new(404, "not_found", "Not found"));
    }
    let now = Timestamp::now();
    match method {
        "GET" => list(runtime),
        "PUT" => {
            let update: Update = decode(body, 16_384)?;
            let key = runtime.store.save(
                &update.id,
                Policy {
                    disabled: update.disabled,
                    expires_at: update.expires_at,
                },
            )?;
            let mut result = key_json(key, now);
            result["server_time"] = json!(now);
            Ok(json_response(200, &result))
        }
        _ => Err(Error::new(405, "method_not_allowed", "Method not allowed")),
    }
}

fn list(runtime: &Runtime) -> Result<Value> {
    let now = Timestamp::now();
    let entries: Vec<_> = runtime
        .store
        .list()?
        .into_iter()
        .map(|key| key_json(key, now))
        .collect();
    Ok(json_response(
        200,
        &json!({"keys": entries, "server_time": now}),
    ))
}

fn key_json(key: KeyEntry, now: Timestamp) -> Value {
    json!({"id": key.id, "key": key.key, "status": key.policy.status(now), "policy": key.policy})
}

fn decode<T: serde::de::DeserializeOwned>(body: &str, limit: usize) -> Result<T> {
    if body.len() > limit {
        return Err(INVALID_REQUEST);
    }
    let bytes = STANDARD.decode(body).map_err(|_| INVALID_REQUEST)?;
    serde_json::from_slice(&bytes).map_err(|_| INVALID_REQUEST)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sync {
    keys: Vec<String>,
    #[serde(default)]
    allow_empty: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    id: String,
    disabled: bool,
    #[serde(deserialize_with = "required_expiry")]
    expires_at: Option<Timestamp>,
}

fn required_expiry<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Timestamp>, D::Error> {
    Option::<Timestamp>::deserialize(deserializer)
}

pub fn json_response(status: u16, value: &Value) -> Value {
    response(
        status,
        "application/json; charset=utf-8",
        value.to_string().as_bytes(),
    )
}

fn response(status: u16, mime: &str, bytes: &[u8]) -> Value {
    json!({"StatusCode": status, "Headers": {
        "Content-Type": [mime], "Cache-Control": ["no-store"],
        "X-Content-Type-Options": ["nosniff"], "Referrer-Policy": ["no-referrer"],
        "Content-Security-Policy": ["default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'"]
    }, "Body": STANDARD.encode(bytes)})
}
