use std::{fs, path::Path, sync::Mutex, time::Duration};

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;

use crate::{
    error::{Error, Result, SYNC_REQUIRED, UNAVAILABLE},
    identity::KeyIdentity,
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Policy {
    pub disabled: bool,
    pub expires_at: Option<Timestamp>,
}

impl Policy {
    pub fn status(&self, now: Timestamp) -> &'static str {
        if self.disabled {
            "disabled"
        } else if self.expires_at.is_some_and(|expiry| now >= expiry) {
            "expired"
        } else {
            "active"
        }
    }
}

pub struct Store(Mutex<Connection>);

#[derive(Serialize)]
pub struct KeyEntry {
    pub id: String,
    pub key: String,
    pub policy: Policy,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|_| UNAVAILABLE)?;
        }
        // A missing policy file during initial installation is an empty policy set.
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;")?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 3 {
            return Err(Error::new(
                503,
                "unsupported_database",
                "Policy database requires a newer plugin",
            ));
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS key_policies (
            key_id TEXT PRIMARY KEY,
            disabled INTEGER NOT NULL CHECK(disabled IN (0, 1)),
            expires_at TEXT
        );
        CREATE TABLE IF NOT EXISTS key_catalog (
            key_id TEXT PRIMARY KEY,
            caller_scope TEXT NOT NULL UNIQUE,
            masked TEXT NOT NULL,
            in_config INTEGER NOT NULL CHECK(in_config IN (0, 1))
        );",
        )?;
        // Remove the obsolete edit revision without discarding existing rules.
        if conn
            .prepare("SELECT 1 FROM pragma_table_info('key_policies') WHERE name='revision'")?
            .exists([])?
        {
            conn.execute_batch("ALTER TABLE key_policies DROP COLUMN revision;")?;
        }
        // Version 1 policies lack caller_scope mappings. Leave that migration
        // pending across restarts until the UI supplies a complete key snapshot.
        if version != 1 {
            conn.execute_batch("PRAGMA user_version = 3;")?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .map_err(|_| UNAVAILABLE)?;
        }
        Ok(Self(Mutex::new(conn)))
    }

    pub fn sync(&self, keys: &[String]) -> Result<()> {
        let identities: Vec<_> = keys
            .iter()
            .filter_map(|key| KeyIdentity::from_value(key))
            .collect();
        let mut conn = self.0.lock().map_err(|_| UNAVAILABLE)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE key_catalog SET in_config=0", [])?;
        for key in identities {
            tx.execute(
                "INSERT INTO key_catalog (key_id, caller_scope, masked, in_config)
                VALUES (?1, ?2, ?3, 1) ON CONFLICT(key_id) DO UPDATE SET
                masked=excluded.masked, in_config=1",
                params![key.id, key.caller_scope, key.masked],
            )?;
        }
        // Keep old identities and policies when a key disappears from CPA.
        // Re-adding the same key must not reset its expiration or disabled state.
        tx.execute_batch("PRAGMA user_version = 3;")?;
        tx.commit()?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<KeyEntry>> {
        let conn = self.0.lock().map_err(|_| UNAVAILABLE)?;
        let mut statement = conn.prepare(
            "SELECT k.key_id, k.masked, COALESCE(p.disabled, 0), p.expires_at
            FROM key_catalog k LEFT JOIN key_policies p ON p.key_id=k.key_id
            WHERE k.in_config=1 ORDER BY k.masked, k.key_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (id, key, disabled, expiry) = row?;
            Ok(KeyEntry {
                id,
                key,
                policy: decode_policy(disabled, expiry)?,
            })
        })
        .collect()
    }

    pub fn policy_for_scope(&self, scope: &str) -> Result<Policy> {
        let conn = self.0.lock().map_err(|_| UNAVAILABLE)?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 2 {
            return Err(SYNC_REQUIRED);
        }
        // Catalog membership is for the UI only. CPA authenticates keys, and a
        // newly added key with no rule must work before the next UI synchronization.
        let id: Option<String> = conn
            .query_row(
                "SELECT key_id FROM key_catalog WHERE caller_scope=?1",
                [scope],
                |row| row.get(0),
            )
            .optional()?;
        match id {
            Some(id) => read_policy(&conn, &id),
            None => Ok(Policy::default()),
        }
    }

    pub fn save(&self, id: &str, policy: Policy) -> Result<KeyEntry> {
        let conn = self.0.lock().map_err(|_| UNAVAILABLE)?;
        let key = conn
            .query_row(
                "SELECT masked FROM key_catalog WHERE key_id=?1 AND in_config=1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(Error::new(
                404,
                "key_not_found",
                "Key is not in the synchronized CPA key list",
            ))?;
        conn.execute(
            "INSERT INTO key_policies (key_id, disabled, expires_at)
            VALUES (?1, ?2, ?3) ON CONFLICT(key_id) DO UPDATE SET
            disabled=excluded.disabled, expires_at=excluded.expires_at",
            params![
                id,
                policy.disabled,
                policy.expires_at.map(|date| date.to_string()),
            ],
        )?;
        Ok(KeyEntry {
            id: id.into(),
            key,
            policy,
        })
    }
}

fn read_policy(conn: &Connection, id: &str) -> Result<Policy> {
    let row: Option<(bool, Option<String>)> = conn
        .query_row(
            "SELECT disabled, expires_at FROM key_policies WHERE key_id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((disabled, expiry)) = row else {
        return Ok(Policy::default());
    };
    decode_policy(disabled, expiry)
}

fn decode_policy(disabled: bool, expiry: Option<String>) -> Result<Policy> {
    let expires_at = expiry
        .map(|value| value.parse::<Timestamp>().map_err(|_| UNAVAILABLE))
        .transpose()?;
    Ok(Policy {
        disabled,
        expires_at,
    })
}
