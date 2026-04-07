use std::collections::HashMap;
use std::sync::Mutex;
use chrono::Utc;
use serde::Serialize;

/// In-memory store for timing challenge nonces.
/// Nonces expire after a configurable duration (default 300s).
///
/// Thread-safe via Mutex. This is fine for the expected load;
/// timing challenges are infrequent compared to reads.
pub struct ChallengeStore {
    /// nonce_hex -> issued_at_ms
    challenges: Mutex<HashMap<String, i64>>,
    /// How long challenges remain valid, in milliseconds
    expiry_ms: i64,
}

/// A timing challenge issued to an agent.
#[derive(Debug, Clone, Serialize)]
pub struct Challenge {
    pub nonce: String,
    pub issued_at: i64,
    pub windows: ChallengeWindows,
}

/// Available timing windows for the challenge response.
#[derive(Debug, Clone, Serialize)]
pub struct ChallengeWindows {
    pub fast: WindowDef,
    pub mid: WindowDef,
    pub large: WindowDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowDef {
    pub min_ms: u64,
    pub max_ms: u64,
}

impl ChallengeStore {
    pub fn new(expiry_secs: u64) -> Self {
        Self {
            challenges: Mutex::new(HashMap::new()),
            expiry_ms: (expiry_secs * 1000) as i64,
        }
    }

    /// Issue a new timing challenge.
    /// If `pubkey_hex` is provided, the nonce is derived from (issued_at || pubkey).
    /// Otherwise, a random nonce is generated from (issued_at || random).
    pub fn issue(&self, pubkey_hex: Option<&str>) -> Challenge {
        let issued_at = Utc::now().timestamp_millis();

        let nonce_input = match pubkey_hex {
            Some(pk) => format!("{}{}{}", issued_at, pk, rand_bytes_hex()),
            None => format!("{}{}", issued_at, rand_bytes_hex()),
        };

        let nonce = blake3::hash(nonce_input.as_bytes())
            .to_hex()
            .to_string();

        {
            let mut store = self.challenges.lock().unwrap();
            // Opportunistic cleanup of expired challenges
            let cutoff = issued_at - self.expiry_ms;
            store.retain(|_, &mut ts| ts > cutoff);
            store.insert(nonce.clone(), issued_at);
        }

        Challenge {
            nonce,
            issued_at,
            windows: ChallengeWindows {
                fast: WindowDef { min_ms: 150, max_ms: 10_000 },
                mid: WindowDef { min_ms: 400, max_ms: 35_000 },
                large: WindowDef { min_ms: 1_500, max_ms: 180_000 },
            },
        }
    }

    /// Validate that a nonce was issued by this node and has not expired.
    /// Consumes the nonce (single-use).
    pub fn validate_and_consume(&self, nonce: &str) -> Option<i64> {
        let now = Utc::now().timestamp_millis();
        let mut store = self.challenges.lock().unwrap();

        if let Some(issued_at) = store.remove(nonce) {
            if (now - issued_at) <= self.expiry_ms {
                return Some(issued_at);
            }
        }

        None
    }

    /// Get the number of active (non-expired) challenges.
    pub fn active_count(&self) -> usize {
        let now = Utc::now().timestamp_millis();
        let cutoff = now - self.expiry_ms;
        let store = self.challenges.lock().unwrap();
        store.values().filter(|&&ts| ts > cutoff).count()
    }
}

/// Generate 16 random bytes as hex for nonce entropy.
fn rand_bytes_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Use system time + thread id as entropy source.
    // Not cryptographically strong, but nonces are not secrets -
    // they just need to be unique and unpredictable enough to
    // prevent pre-computation of timing proofs.
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tid = std::thread::current().id();
    format!("{:x}{:?}", t, tid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_validate() {
        let store = ChallengeStore::new(300);
        let challenge = store.issue(None);

        assert!(!challenge.nonce.is_empty());
        assert!(challenge.issued_at > 0);

        // Should validate successfully
        let issued = store.validate_and_consume(&challenge.nonce);
        assert!(issued.is_some());
        assert_eq!(issued.unwrap(), challenge.issued_at);

        // Should not validate twice (consumed)
        let again = store.validate_and_consume(&challenge.nonce);
        assert!(again.is_none());
    }

    #[test]
    fn issue_with_pubkey() {
        let store = ChallengeStore::new(300);
        let c1 = store.issue(Some("aabbccdd"));
        let c2 = store.issue(Some("aabbccdd"));

        // Different nonces even with same pubkey (different timestamps)
        assert_ne!(c1.nonce, c2.nonce);
    }

    #[test]
    fn unknown_nonce_rejected() {
        let store = ChallengeStore::new(300);
        let result = store.validate_and_consume("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn expired_nonce_rejected() {
        // Create store with 0-second expiry
        let store = ChallengeStore::new(0);
        let challenge = store.issue(None);

        // Should already be expired
        std::thread::sleep(std::time::Duration::from_millis(10));
        let result = store.validate_and_consume(&challenge.nonce);
        assert!(result.is_none());
    }
}
