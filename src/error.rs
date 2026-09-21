use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub struct Error {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub const fn new(status: u16, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }

    pub fn json(self) -> Value {
        json!({"error": {"code": self.code, "message": self.message}})
    }
}

pub const UNAVAILABLE: Error = Error::new(
    503,
    "storage_unavailable",
    "Key policy storage is unavailable",
);
pub const INVALID_REQUEST: Error = Error::new(400, "invalid_request", "Invalid request");
pub const SYNC_REQUIRED: Error = Error::new(
    503,
    "key_sync_required",
    "Open the key-expiration page to synchronize keys after upgrading",
);

impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        UNAVAILABLE
    }
}
