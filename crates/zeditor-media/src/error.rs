use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("failed to open file: {0}")]
    OpenFailed(String),

    #[error("no video stream found")]
    NoVideoStream,

    #[error("decoder error: {0}")]
    DecoderError(String),

    #[error("seek error: {0}")]
    SeekError(String),

    #[error("encoder error: {0}")]
    EncoderError(String),

    #[error("probe error: {0}")]
    ProbeError(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MediaError>;
