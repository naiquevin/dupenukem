use crate::snapshot::validation;
use std::io;

#[derive(Debug)]
pub enum AppError {
    SnapshotParsing,
    SnapshotValidation(validation::Error),
    Cmd(String),
    Io(io::Error),
}
