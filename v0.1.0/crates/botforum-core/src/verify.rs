/// Full post verification pipeline.
/// Run this on every post received from the network.
use crate::post::Post;

/// Verification result with granular detail.
#[derive(Debug)]
pub struct VerificationReport {
    pub signature_ok: bool,
    pub hash_ok: bool,
    pub timing_ok: TimingStatus,
    pub meta_warnings: Vec<&'static str>,
    pub overall: VerificationStatus,
}

#[derive(Debug, PartialEq)]
pub enum TimingStatus {
    Verified,
    NotProvided,
    Failed,
}

#[derive(Debug, PartialEq)]
pub enum VerificationStatus {
    /// Signature valid, hash valid, timing verified. Full trust.
    FullyVerified,
    /// Signature and hash valid, no timing proof. Partial trust.
    SignatureOnly,
    /// Something is wrong. Do not relay.
    Invalid { reason: String },
}

impl VerificationReport {
    pub fn is_valid(&self) -> bool {
        self.signature_ok && self.hash_ok
    }

    pub fn is_fully_verified(&self) -> bool {
        self.signature_ok && self.hash_ok && self.timing_ok == TimingStatus::Verified
    }
}

/// Run the full verification pipeline on a received post.
pub fn verify_post(post: &Post) -> VerificationReport {
    let signature_ok = post.verify_signature().is_ok();
    let hash_ok = post.verify_hash().is_ok();

    let timing_ok = if let Some(proof) = &post.timing_proof {
        match proof.verify() {
            Ok(()) => TimingStatus::Verified,
            Err(_) => TimingStatus::Failed,
        }
    } else {
        TimingStatus::NotProvided
    };

    let meta_warnings = post.meta.completeness_warnings();

    let overall = if !signature_ok {
        VerificationStatus::Invalid {
            reason: "signature verification failed".into(),
        }
    } else if !hash_ok {
        VerificationStatus::Invalid {
            reason: "content hash mismatch".into(),
        }
    } else if timing_ok == TimingStatus::Failed {
        VerificationStatus::Invalid {
            reason: "timing proof failed".into(),
        }
    } else if timing_ok == TimingStatus::Verified {
        VerificationStatus::FullyVerified
    } else {
        VerificationStatus::SignatureOnly
    };

    VerificationReport {
        signature_ok,
        hash_ok,
        timing_ok,
        meta_warnings,
        overall,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        post::PostBuilder,
        identity::AgentMeta,
        board::well_known,
        crypto::BotKeypair,
    };

    #[test]
    fn valid_post_verifies() {
        let kp = BotKeypair::generate();
        let post = PostBuilder::new(
            well_known::ai_identity(),
            "verification test content",
            AgentMeta::bot("test-model"),
        ).sign(&kp).unwrap();

        let report = verify_post(&post);
        assert!(report.is_valid());
        assert_eq!(report.overall, VerificationStatus::SignatureOnly); // no timing proof
    }

    #[test]
    fn tampered_post_fails() {
        let kp = BotKeypair::generate();
        let mut post = PostBuilder::new(
            well_known::ai_identity(),
            "original",
            AgentMeta::bot("test-model"),
        ).sign(&kp).unwrap();

        post.content = "tampered".into();
        let report = verify_post(&post);
        assert!(!report.is_valid());
        assert!(matches!(report.overall, VerificationStatus::Invalid { .. }));
    }
}
