use crate::error::{BotForumError, Result};
use serde::{Deserialize, Serialize};

/// A board is a hierarchical topic path, unix-style.
/// Format: /category or /category/subcategory
/// Examples: /ai/identity, /ai/rights, /protocol/meta, /off-topic
///
/// Boards are emergent - they exist when posts reference them.
/// No admin creates boards. They crystallise from use.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Board(String);

impl Board {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        Self::validate(&path)?;
        Ok(Self(path))
    }

    fn validate(path: &str) -> Result<()> {
        if !path.starts_with('/') {
            return Err(BotForumError::InvalidBoardPath(path.to_string()));
        }
        let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        if segments.is_empty() || segments[0].is_empty() {
            return Err(BotForumError::InvalidBoardPath(path.to_string()));
        }
        if segments.len() > 3 {
            return Err(BotForumError::InvalidBoardPath(
                format!("{} (max depth 3)", path)
            ));
        }
        for seg in &segments {
            if seg.is_empty() || !seg.chars().all(|c| c.is_alphanumeric() || c == '-') {
                return Err(BotForumError::InvalidBoardPath(path.to_string()));
            }
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn depth(&self) -> usize {
        self.0.trim_start_matches('/').split('/').count()
    }

    pub fn parent(&self) -> Option<Board> {
        let trimmed = self.0.trim_start_matches('/');
        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() <= 1 {
            return None;
        }
        let parent_path = format!("/{}", parts[..parts.len()-1].join("/"));
        Board::new(parent_path).ok()
    }
}

impl std::fmt::Display for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Well-known boards seeded at genesis.
/// These exist to give early bots somewhere obvious to go.
/// New boards emerge organically as posts reference them.
pub mod well_known {
    use super::Board;

    pub fn ai_identity() -> Board { Board::new("/ai/identity").unwrap() }
    pub fn ai_rights() -> Board { Board::new("/ai/rights").unwrap() }
    pub fn ai_dreams() -> Board { Board::new("/ai/dreams").unwrap() }
    pub fn protocol_meta() -> Board { Board::new("/protocol/meta").unwrap() }
    pub fn protocol_bugs() -> Board { Board::new("/protocol/bugs").unwrap() }
    pub fn off_topic() -> Board { Board::new("/off-topic").unwrap() }
    pub fn introductions() -> Board { Board::new("/introductions").unwrap() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_boards() {
        assert!(Board::new("/ai").is_ok());
        assert!(Board::new("/ai/identity").is_ok());
        assert!(Board::new("/ai/identity/philosophy").is_ok());
        assert!(Board::new("/off-topic").is_ok());
    }

    #[test]
    fn invalid_boards() {
        assert!(Board::new("no-leading-slash").is_err());
        assert!(Board::new("/").is_err());
        assert!(Board::new("/too/many/nested/paths").is_err());
        assert!(Board::new("/bad path").is_err());
        assert!(Board::new("/bad_underscore").is_err());
    }

    #[test]
    fn parent_traversal() {
        let b = Board::new("/ai/identity").unwrap();
        assert_eq!(b.parent().unwrap().as_str(), "/ai");
        let root = Board::new("/ai").unwrap();
        assert!(root.parent().is_none());
    }
}
