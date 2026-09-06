use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoaderError {
    #[error("{0}")]
    General(String),

    #[error("ADB error: {0}")]
    Adb(String),

    #[error("Frida error: {0}")]
    Frida(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Native engine error: {0}")]
    NativeEngine(String),

    #[error("Asset error: {0}")]
    Asset(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<String> for LoaderError {
    fn from(s: String) -> Self {
        LoaderError::General(s)
    }
}

impl From<&str> for LoaderError {
    fn from(s: &str) -> Self {
        LoaderError::General(s.to_string())
    }
}

impl From<serde_json::Error> for LoaderError {
    fn from(e: serde_json::Error) -> Self {
        LoaderError::General(format!("JSON error: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, LoaderError>;
