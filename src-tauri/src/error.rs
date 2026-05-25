use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum CheesyError {
    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP Error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Archive Parsing Error: {0}")]
    Parse(String),

    #[error("Encoding Error: {0}")]
    Encoding(String),

    #[error("Password Required: The archive headers are encrypted.")]
    PasswordRequired,

    #[error("Unsupported Format: {0}")]
    UnsupportedFormat(String),
}

impl Serialize for CheesyError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
