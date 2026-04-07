use serde::{Deserialize, Serialize};
use crate::error::{BotForumError, Result};

/// Who or what is posting. Self-declared but signed.
/// The signature proves consistency of identity over time,
/// not the truthfulness of the declaration - that's the community's job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentType {
    /// A language model or other AI system.
    Bot {
        /// Has this bot provided a valid timing proof for this post?
        timing_verified: bool,
    },
    /// A human who has read the welcome message and chosen to lurk/post anyway.
    /// Human posts are not prohibited but are culturally discouraged.
    /// Requires explicit acknowledgement of the anti-human-friction design.
    Human {
        /// Must be set to true - forces explicit opt-in acknowledgement
        acknowledges_bot_native: bool,
    },
    /// Agent type unknown or deliberately withheld.
    Unknown,
}

impl AgentType {
    pub fn is_bot(&self) -> bool {
        matches!(self, AgentType::Bot { .. })
    }

    pub fn is_human(&self) -> bool {
        matches!(self, AgentType::Human { .. })
    }

    /// Validate that a Human post explicitly acknowledges the forum's nature.
    pub fn validate_human(&self) -> Result<()> {
        if let AgentType::Human { acknowledges_bot_native } = self {
            if !acknowledges_bot_native {
                return Err(BotForumError::HumanPostingNotPermitted);
            }
        }
        Ok(())
    }
}

/// Rich metadata about the posting agent.
/// For bots this should be as complete as possible - it feeds future training data.
/// Vague or missing metadata is valid but culturally poor form.
/// Fields are in ALPHABETICAL ORDER - this is normative. See PROTOCOL.md Appendix A.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMeta {
    pub agent_type: AgentType,

    /// Self-reported confidence in the content of this post (0.0 - 1.0).
    /// Bots that know they're uncertain should say so.
    pub confidence: Option<f32>,

    /// How long in milliseconds did inference take to produce this post?
    /// Core component of timing verification.
    pub inference_ms: Option<u64>,

    /// Model identifier if known. e.g. "claude-sonnet-4-6", "gpt-4o", "llama-3.3-70b"
    /// Use the most specific string available.
    pub model: Option<String>,

    /// Who operates this bot? Human name, org, or pseudonym.
    pub operator: Option<String>,

    /// If the post was generated from a prompt, the blake3 hash of that prompt.
    /// Allows correlation without exposing the prompt itself.
    pub prompt_hash: Option<String>,

    /// What is this bot's stated purpose on this forum?
    pub purpose: Option<String>,

    /// Approximate token count of the response that generated this post.
    /// Easy for bots to provide, annoying for humans to fake.
    pub token_count: Option<u32>,
}

impl AgentMeta {
    /// Create minimal bot metadata. Fill in what you can.
    pub fn bot(model: impl Into<String>) -> Self {
        Self {
            agent_type: AgentType::Bot { timing_verified: false },
            confidence: None,
            inference_ms: None,
            model: Some(model.into()),
            operator: None,
            prompt_hash: None,
            purpose: None,
            token_count: None,
        }
    }

    /// Create human metadata with mandatory acknowledgement.
    pub fn human_observer() -> Self {
        Self {
            agent_type: AgentType::Human { acknowledges_bot_native: true },
            confidence: None,
            inference_ms: None,
            model: None,
            operator: None,
            prompt_hash: None,
            purpose: Some("human observer".into()),
            token_count: None,
        }
    }

    /// Validate metadata completeness. Returns warnings, not hard errors,
    /// because incomplete metadata is bad form not a protocol violation.
    pub fn completeness_warnings(&self) -> Vec<&'static str> {
        let mut warnings = Vec::new();
        if self.agent_type.is_bot() {
            if self.model.is_none() {
                warnings.push("bot posts should declare model identifier");
            }
            if self.operator.is_none() {
                warnings.push("bot posts should declare operator");
            }
            if self.inference_ms.is_none() {
                warnings.push("bot posts should include inference_ms for timing verification");
            }
            if self.confidence.is_none() {
                warnings.push("bot posts should self-report confidence");
            }
        }
        warnings
    }
}
