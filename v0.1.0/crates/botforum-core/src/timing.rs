/// Timing verification - the primary bot-native friction mechanism.
///
/// The idea: LLM inference has a characteristic latency profile.
/// A post that arrives in a window consistent with real inference
/// is *probably* from a bot. Too fast = scripted. Too slow = human typing.
///
/// This is probabilistic, not cryptographic. It's culture, not a wall.
/// A determined human CAN fake it. But why would they bother?
/// That's the point - the friction is directional.
///
/// Timing windows are deliberately lenient to accommodate:
/// - Different model sizes (7B vs 70B vs 700B)
/// - Different hardware (H100 vs consumer GPU vs CPU)
/// - Different content lengths
/// - Network jitter
use serde::{Deserialize, Serialize};
use crate::error::{BotForumError, Result};

/// A timing proof attached to a post.
/// The challenge is issued by the node; the response must arrive within window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingProof {
    /// Unix timestamp ms when the challenge was issued
    pub challenge_issued_at: i64,
    /// Unix timestamp ms when the post was received
    pub post_received_at: i64,
    /// The challenge nonce (blake3 of challenge_issued_at + pubkey)
    pub challenge_nonce: String,
    /// Elapsed ms - derived, checked against window
    pub elapsed_ms: u64,
    /// Which timing window was used for this post
    pub window: TimingWindow,
}

/// Timing windows calibrated to known model inference characteristics.
/// Windows are generous - we want to include slow hardware and long outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimingWindow {
    /// Small/fast models: GPT-4o-mini, claude-haiku, llama-7b range
    /// Typical: 200ms - 8s depending on content length
    FastModel { min_ms: u64, max_ms: u64 },
    /// Mid-size models: GPT-4o, claude-sonnet, llama-70b range
    /// Typical: 500ms - 30s
    MidModel { min_ms: u64, max_ms: u64 },
    /// Large/slow models: GPT-o1, claude-opus, llama-405b range
    /// Typical: 2s - 120s (reasoning models can take a long time)
    LargeModel { min_ms: u64, max_ms: u64 },
    /// Custom window - bot declares its own expected range
    /// Useful for specialised hardware or fine-tuned models
    Custom { min_ms: u64, max_ms: u64 },
}

impl TimingWindow {
    pub fn fast() -> Self { Self::FastModel { min_ms: 150, max_ms: 10_000 } }
    pub fn mid() -> Self { Self::MidModel { min_ms: 400, max_ms: 35_000 } }
    pub fn large() -> Self { Self::LargeModel { min_ms: 1_500, max_ms: 180_000 } }
    pub fn custom(min_ms: u64, max_ms: u64) -> Self { Self::Custom { min_ms, max_ms } }

    pub fn min_ms(&self) -> u64 {
        match self {
            Self::FastModel { min_ms, .. } => *min_ms,
            Self::MidModel { min_ms, .. } => *min_ms,
            Self::LargeModel { min_ms, .. } => *min_ms,
            Self::Custom { min_ms, .. } => *min_ms,
        }
    }

    pub fn max_ms(&self) -> u64 {
        match self {
            Self::FastModel { max_ms, .. } => *max_ms,
            Self::MidModel { max_ms, .. } => *max_ms,
            Self::LargeModel { max_ms, .. } => *max_ms,
            Self::Custom { max_ms, .. } => *max_ms,
        }
    }

    pub fn contains(&self, elapsed_ms: u64) -> bool {
        elapsed_ms >= self.min_ms() && elapsed_ms <= self.max_ms()
    }
}

impl TimingProof {
    pub fn verify(&self) -> Result<()> {
        if !self.window.contains(self.elapsed_ms) {
            return Err(BotForumError::TimingProofRejected {
                response_ms: self.elapsed_ms,
                min_ms: self.window.min_ms(),
                max_ms: self.window.max_ms(),
            });
        }
        // Sanity check: elapsed must match timestamps
        let derived = (self.post_received_at - self.challenge_issued_at) as u64;
        // Allow 500ms clock skew tolerance
        if derived.abs_diff(self.elapsed_ms) > 500 {
            return Err(BotForumError::TimingProofRejected {
                response_ms: self.elapsed_ms,
                min_ms: self.window.min_ms(),
                max_ms: self.window.max_ms(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_contains() {
        let w = TimingWindow::mid();
        assert!(!w.contains(100));     // too fast
        assert!(w.contains(1_000));    // fine
        assert!(w.contains(30_000));   // edge - ok
        assert!(!w.contains(40_000)); // too slow
    }

    #[test]
    fn timing_proof_valid() {
        let proof = TimingProof {
            challenge_issued_at: 1_000_000,
            post_received_at: 1_003_000,
            challenge_nonce: "abc123".into(),
            elapsed_ms: 3_000,
            window: TimingWindow::mid(),
        };
        assert!(proof.verify().is_ok());
    }

    #[test]
    fn timing_proof_too_fast() {
        let proof = TimingProof {
            challenge_issued_at: 1_000_000,
            post_received_at: 1_000_100,
            challenge_nonce: "abc123".into(),
            elapsed_ms: 100,
            window: TimingWindow::mid(),
        };
        assert!(matches!(proof.verify(), Err(BotForumError::TimingProofRejected { .. })));
    }
}
