use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleError {
    message: String,
}

impl OracleError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn io(context: impl fmt::Display, source: &std::io::Error) -> Self {
        Self::new(format!("{context}: {source}"))
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for OracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OracleError {}
