// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Shared generic utilities (timestamp formatting, content fingerprints,
//! uid generation). Survives the knowledge-domain retirement — used by the
//! task scheduler, pipes and other core surfaces.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// sha256 hex of the UTF-8 encoding of `parts` joined with `\x1f`.
pub fn fingerprint(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            hasher.update([0x1f]);
        }
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

pub fn new_source_uid() -> String {
    Uuid::new_v4().to_string()
}

pub fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

/// Timestamp format used across SQLite TEXT columns (RFC3339 micros).
pub fn format_ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_hex() {
        let a = fingerprint(&["x", "y"]);
        let b = fingerprint(&["x", "y"]);
        let c = fingerprint(&["x", "z"]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn format_ts_is_rfc3339_with_micros() {
        let t = Utc::now();
        let s = format_ts(t);
        assert!(s.ends_with('Z'));
        assert!(s.contains('.'));
        assert!(s.contains(':'));
    }
}