use crate::snapshot::validation;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Failed to parse snapshot")]
    SnapshotParsing,
    #[error("Snapshot validation failed: {0:?}")]
    SnapshotValidation(validation::Error),
    #[error("Command error: {0}")]
    Cmd(String),
    #[error("IO error: {0}")]
    Io(io::Error),
    #[error("Failed to perform file operation: {0}")]
    Fs(String),
    #[error("Failed to parse checksum")]
    ChecksumParsing,
}
