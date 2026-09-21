use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Deserialize)]
#[serde(default)]
pub struct Settings {
    pub database_path: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            database_path: "plugins/key-expiration.sqlite3".into(),
        }
    }
}

impl Settings {
    pub fn parse(yaml: &[u8]) -> Result<Self> {
        let invalid = Error::new(
            503,
            "invalid_settings",
            "Invalid key-expiration plugin configuration",
        );
        let settings: Self = serde_yaml_ng::from_slice(yaml).map_err(|_| invalid)?;
        if settings.database_path.as_os_str().is_empty() {
            return Err(invalid);
        }
        Ok(settings)
    }
}
