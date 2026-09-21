use std::fs;

use base64::{Engine, engine::general_purpose::STANDARD};
use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::{
    Plugin, Runtime,
    config::Settings,
    identity::KeyIdentity,
    intercept,
    management::{API_PATH, SYNC_PATH},
    store::{Policy, Store},
};

const KEY: &str = "sk-test-key-0001";
const SCOPE: &str = "6df3f70e71486751d90152257b4a86ead883a8861692be5221363c2077dd540f";

struct Fixture {
    dir: TempDir,
    plugin: Plugin,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            dir: tempfile::tempdir().unwrap(),
            plugin: Plugin::default(),
        };
        fixture.register();
        fixture.sync(&[KEY]);
        fixture
    }

    fn settings(&self) -> Settings {
        Settings {
            database_path: self.dir.path().join("rules.sqlite3"),
        }
    }

    fn register(&self) -> Value {
        self.plugin.call("plugin.register", json!({
            "schema_version": 6,
            "config_yaml": STANDARD.encode(json!({"database_path": self.settings().database_path}).to_string())
        })).unwrap()
    }

    fn management(&self, method: &str, path: &str, body: Value) -> (u64, Value) {
        let result = self
            .plugin
            .call(
                "management.handle",
                json!({
                    "Method": method, "Path": path, "Body": STANDARD.encode(body.to_string())
                }),
            )
            .unwrap();
        let body = STANDARD.decode(result["Body"].as_str().unwrap()).unwrap();
        (
            result["StatusCode"].as_u64().unwrap(),
            serde_json::from_slice(&body).unwrap(),
        )
    }

    fn sync(&self, keys: &[&str]) -> Value {
        let (status, result) = self.management(
            "POST",
            SYNC_PATH,
            json!({"keys": keys, "allow_empty": true}),
        );
        assert_eq!(status, 200);
        result
    }

    fn runtime<T>(&self, call: impl FnOnce(&Runtime) -> T) -> T {
        let guard = self.plugin.runtime.read().unwrap();
        call(guard.as_ref().ok().unwrap())
    }
}

fn now() -> Timestamp {
    "2026-09-21T08:00:00Z".parse().unwrap()
}
fn identity() -> KeyIdentity {
    KeyIdentity::from_value(KEY).unwrap()
}

#[test]
fn expiration_boundary_and_disable_are_independent() {
    let mut policy = Policy {
        expires_at: Some(now()),
        ..Policy::default()
    };
    assert_eq!(
        policy.status(now() - SignedDuration::from_nanos(1)),
        "active"
    );
    assert_eq!(policy.status(now()), "expired");
    policy.disabled = true;
    policy.expires_at = None;
    assert_eq!(policy.status(now()), "disabled");
    policy.disabled = false;
    assert_eq!(policy.status(now()), "active");
}

#[test]
fn interception_only_rejects_restricted_keys_and_never_rewrites_requests() {
    let fixture = Fixture::new();
    assert_eq!(identity().caller_scope, SCOPE); // Fixed CPA-generated identity vector.
    assert_eq!(
        fixture.register()["capabilities"],
        json!({"request_interceptor": true, "management_api": true})
    );
    fixture.runtime(|runtime| {
        for stream in [false, true] {
            for scope in [SCOPE, "new-key-not-yet-synced", ""] {
                let request = json!({
                    "RequestID": "req-1", "TraceID": "trace-1", "SourceFormat": "openai",
                    "Model": "provider/model", "RequestedModel": "alias", "Stream": stream,
                    "Headers": {"X-Custom": ["one", "two"], "Authorization": ["Bearer upstream"]},
                    "Body": STANDARD.encode(b" {\"messages\":[{\"role\":\"user\",\"content\":\"hello\"}]}\n"),
                    "Metadata": {"caller_scope": scope, "custom": {"nested": [1, 2]}}
                });
                let original = request.clone();
                assert_eq!(intercept::before(runtime, &request, now()).unwrap(), json!({}));
                assert_eq!(request, original);
                assert_eq!(fixture.plugin.call("request.intercept_after", request).unwrap(), json!({}));
            }
        }
        let request = json!({"Metadata": {"caller_scope": SCOPE}});
        for policy in [
            Policy { expires_at: Some(now()), ..Policy::default() },
            Policy { disabled: true, ..Policy::default() },
            Policy::default(),
        ] {
            let rejected = policy.status(now()) != "active";
            runtime.store.save(&identity().id, policy).unwrap();
            let result = intercept::before(runtime, &request, now()).unwrap();
            if rejected {
                assert_eq!(result["Terminate"], true);
                assert_eq!(result["StatusCode"], 401);
                assert!(result.get("Body").is_none());
                assert!(result.get("Headers").is_none());
                assert_eq!(fixture.plugin.call("request.intercept_after", request.clone()).unwrap(), json!({}));
            } else { assert_eq!(result, json!({})); }
        }
    });
}

#[test]
fn synchronization_preserves_rules_and_never_stores_plaintext_keys() {
    let fixture = Fixture::new();
    fixture.runtime(|runtime| {
        runtime
            .store
            .save(
                &identity().id,
                Policy {
                    disabled: true,
                    ..Policy::default()
                },
            )
            .unwrap();
    });
    let list = fixture.sync(&[KEY, KEY, "  sk-test-key-0001  ", ""]);
    assert_eq!(list["keys"].as_array().unwrap().len(), 1);
    assert!(!list.to_string().contains(KEY));
    assert!(fixture.sync(&[])["keys"].as_array().unwrap().is_empty());
    fixture.runtime(|runtime| {
        assert_eq!(
            runtime.store.policy_for_scope(SCOPE).unwrap().status(now()),
            "disabled"
        );
    });
    fixture.plugin.shutdown();
    fixture.register();
    let restored = fixture.sync(&[KEY]);
    assert_eq!(restored["keys"][0]["status"], "disabled");
    fixture.plugin.shutdown();
    for file in fs::read_dir(fixture.dir.path()).unwrap() {
        let bytes = fs::read(file.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(KEY.len())
                .any(|window| window == KEY.as_bytes())
        );
    }
}

#[test]
fn persistence_and_storage_failures_do_not_bypass_rules() {
    let fixture = Fixture::new();
    let policy = Policy {
        disabled: true,
        expires_at: Some(now()),
    };
    fixture.runtime(|runtime| {
        runtime.store.save(&identity().id, policy).unwrap();
    });
    fixture.plugin.shutdown();
    fixture.register();
    fixture.runtime(|runtime| {
        assert_eq!(
            runtime.store.policy_for_scope(SCOPE).unwrap().expires_at,
            Some(now())
        );
    });
    let request = json!({"Metadata": {"caller_scope": SCOPE}});
    assert_eq!(
        fixture
            .plugin
            .call("request.intercept_before", request.clone())
            .unwrap()["StatusCode"],
        401
    );
    fixture.plugin.shutdown();
    fs::write(fixture.settings().database_path, b"corrupt database").unwrap();
    fixture.register();
    assert_eq!(fixture.management("GET", API_PATH, Value::Null).0, 503);
    assert_eq!(
        fixture
            .plugin
            .call("request.intercept_before", request)
            .unwrap()["StatusCode"],
        503
    );
}

#[test]
fn management_validates_sync_and_policy_updates() {
    let fixture = Fixture::new();
    for body in [
        json!({}),
        json!({"keys": [42]}),
        json!({"keys": []}),
        json!({"keys": [" "]}),
    ] {
        assert_eq!(fixture.management("POST", SYNC_PATH, body).0, 400);
    }
    let (_, list) = fixture.management("GET", API_PATH, Value::Null);
    assert_eq!(list["keys"].as_array().unwrap().len(), 1);
    let id = list["keys"][0]["id"].clone();
    let update = json!({"id": id, "disabled": false, "expires_at": "2026-09-21T16:00:00+08:00"});
    let (status, saved) = fixture.management("PUT", API_PATH, update.clone());
    assert_eq!(status, 200);
    assert_eq!(saved["policy"]["expires_at"], "2026-09-21T08:00:00Z");
    let (status, saved) = fixture.management(
        "PUT",
        API_PATH,
        json!({"id": id, "disabled": true, "expires_at": null}),
    );
    assert_eq!(status, 200);
    assert_eq!(
        saved["policy"],
        json!({"disabled": true, "expires_at": null})
    );
    for invalid in [
        json!({"id": id, "disabled": false}),
        json!({"id": id, "disabled": false, "expires_at": "2026-09-21T08:00:00"}),
        json!({"id": id, "disabled": "false", "expires_at": null}),
    ] {
        assert_eq!(fixture.management("PUT", API_PATH, invalid).0, 400);
    }
    fixture.sync(&[]);
    assert_eq!(fixture.management("PUT", API_PATH, update).0, 404);
}

#[test]
fn legacy_policies_require_sync_then_keep_their_original_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE key_policies (key_id TEXT PRIMARY KEY, disabled INTEGER NOT NULL, expires_at TEXT, revision INTEGER NOT NULL); PRAGMA user_version=1;").unwrap();
    conn.execute(
        "INSERT INTO key_policies VALUES (?1, 1, ?2, 7)",
        rusqlite::params![identity().id, now().to_string()],
    )
    .unwrap();
    drop(conn);
    for _ in 0..2 {
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store.policy_for_scope(SCOPE).unwrap_err().code,
            "key_sync_required"
        );
    }
    let store = Store::open(&path).unwrap();
    store.sync(&[KEY.into()]).unwrap();
    let policy = store.policy_for_scope(SCOPE).unwrap();
    assert!(policy.disabled);
    assert_eq!(policy.expires_at, Some(now()));
    store.save(&identity().id, Policy::default()).unwrap();
    drop(store);
    assert_eq!(
        Store::open(&path)
            .unwrap()
            .policy_for_scope(SCOPE)
            .unwrap()
            .status(now()),
        "active"
    );
}
