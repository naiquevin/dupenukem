use crate::snapshot::validation;

#[derive(Debug)]
pub enum AppError {
    SnapshotParsing,
    SnapshotValidation(validation::Error),
    Cmd,
}
