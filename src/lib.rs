mod abi;
mod config;
mod error;
mod identity;
mod intercept;
mod management;
mod store;

use std::sync::RwLock;

use base64::{Engine, engine::general_purpose::STANDARD};
use jiff::Timestamp;
use serde_json::{Value, json};

use config::Settings;
use error::{Error, INVALID_REQUEST, Result, UNAVAILABLE};
use store::Store;

struct Runtime {
    store: Store,
}

impl Runtime {
    fn open(settings: Settings) -> Result<Self> {
        Ok(Self {
            store: Store::open(&settings.database_path)?,
        })
    }
}

struct Plugin {
    runtime: RwLock<Result<Runtime>>,
}

impl Default for Plugin {
    fn default() -> Self {
        Self {
            runtime: RwLock::new(Err(UNAVAILABLE)),
        }
    }
}

impl Plugin {
    fn call(&self, method: &str, request: Value) -> Result<Value> {
        match method {
            "plugin.register" | "plugin.reconfigure" => {
                let settings = request
                    .get("config_yaml")
                    .and_then(Value::as_str)
                    .ok_or(INVALID_REQUEST)
                    .and_then(|encoded| STANDARD.decode(encoded).map_err(|_| INVALID_REQUEST))
                    .and_then(|bytes| Settings::parse(&bytes));
                // Keep the interceptor registered on initialization failure so a
                // storage error cannot silently bypass existing expiration rules.
                let mut runtime = self.runtime.write().map_err(|_| UNAVAILABLE)?;
                *runtime = settings.and_then(Runtime::open);
                Ok(registration())
            }
            "request.intercept_before" => {
                let result = self
                    .with_runtime(|runtime| intercept::before(runtime, &request, Timestamp::now()));
                Ok(result.unwrap_or_else(intercept::reject))
            }
            "request.intercept_after" => Ok(json!({})),
            "management.register" => Ok(management::registration()),
            "management.handle" => {
                let method = request
                    .get("Method")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let path = request
                    .get("Path")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(asset) = management::asset(method, path) {
                    return Ok(asset);
                }
                let result = self.with_runtime(|runtime| {
                    management::handle(
                        runtime,
                        method,
                        path,
                        request
                            .get("Body")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
                });
                Ok(result
                    .unwrap_or_else(|error| management::json_response(error.status, &error.json())))
            }
            "plugin.quiesce" => Ok(json!({})),
            "plugin.shutdown" => {
                self.shutdown();
                Ok(json!({}))
            }
            _ => Err(Error::new(400, "unknown_method", "Unknown plugin method")),
        }
    }

    fn with_runtime<T>(&self, call: impl FnOnce(&Runtime) -> Result<T>) -> Result<T> {
        let guard = self.runtime.read().map_err(|_| UNAVAILABLE)?;
        call(guard.as_ref().map_err(|error| *error)?)
    }

    fn shutdown(&self) {
        if let Ok(mut runtime) = self.runtime.write() {
            *runtime = Err(UNAVAILABLE);
        }
    }
}

fn registration() -> Value {
    json!({"schema_version": 6, "metadata": {
        "Name": "Key Expiration", "Version": env!("CARGO_PKG_VERSION"),
        "Author": "zabbits", "GitHubRepository": env!("CARGO_PKG_REPOSITORY"),
        "ConfigFields": [
            {"Name": "database_path", "Type": "string", "Description": "规则数据库路径（默认 plugins/key-expiration.sqlite3）"}
        ]
    }, "capabilities": {"request_interceptor": true, "management_api": true}})
}

#[cfg(test)]
mod tests;
