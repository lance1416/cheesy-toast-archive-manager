use serde::{Serialize, Serializer};

#[expect(dead_code)] // TODO: Temporary attribute to pacify unused code warnings
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
