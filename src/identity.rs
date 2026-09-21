use sha2::{Digest, Sha256};

pub struct KeyIdentity {
    pub id: String,
    pub caller_scope: String,
    pub masked: String,
}

impl KeyIdentity {
    pub fn from_value(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        let chars: Vec<_> = value.chars().collect();
        let masked = if chars.len() <= 12 {
            "••••••••".into()
        } else {
            format!(
                "{}••••{}",
                chars[..6].iter().collect::<String>(),
                chars[chars.len() - 4..].iter().collect::<String>()
            )
        };
        Some(Self {
            // Retain the original policy IDs so existing expiration rules survive upgrades.
            id: format!("{:x}", Sha256::digest(value.as_bytes())),
            // This prefix is part of CPA's public caller_scope identity convention.
            caller_scope: format!(
                "{:x}",
                Sha256::digest(format!("cli-proxy-api:caller-scope:v1\0{value}"))
            ),
            masked,
        })
    }
}
