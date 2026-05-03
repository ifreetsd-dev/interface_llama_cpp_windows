use std::fmt;

#[derive(Debug)]
pub enum Error {
    Config(String),
    Process(String),
    Vram(String),
    Ui(String),
    Io(String),
}

impl Error {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
    pub fn process(msg: impl Into<String>) -> Self {
        Self::Process(msg.into())
    }
    pub fn vram(msg: impl Into<String>) -> Self {
        Self::Vram(msg.into())
    }
    pub fn ui(msg: impl Into<String>) -> Self {
        Self::Ui(msg.into())
    }
    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(m) => write!(f, "ConfigError: {}", m),
            Self::Process(m) => write!(f, "ProcessError: {}", m),
            Self::Vram(m) => write!(f, "VramError: {}", m),
            Self::Ui(m) => write!(f, "UiError: {}", m),
            Self::Io(m) => write!(f, "IoError: {}", m),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::io(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::config(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;