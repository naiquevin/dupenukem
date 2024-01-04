use crate::error::AppError;
use log::info;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Action<'a> {
    Keep(&'a PathBuf),
    Symlink {
        path: &'a PathBuf,
        source: &'a PathBuf,
        is_no_op: bool,
    },
    Delete {
        path: &'a PathBuf,
        is_no_op: bool,
    },
}

impl<'a> Action<'a> {
    pub fn log(&self, dry_run: &bool) -> Option<String> {
        match self {
            Self::Keep(_) => return None,
            Self::Symlink {
                path,
                source,
                is_no_op,
            } => {
                let mut res = String::from("");
                if *dry_run {
                    res.push_str("[DRY RUN] ");
                }
                if *is_no_op {
                    res.push_str("[NO-OP] ");
                }
                res.push_str(
                    format!(
                        "Replacing file with symlink: {} -> {}",
                        path.display(),
                        // Here we're assuming that the source will never be
                        // None
                        source.display(),
                    )
                    .as_str(),
                );
                Some(res)
            }
            Self::Delete { path, is_no_op } => {
                let mut res = String::from("");
                if *dry_run {
                    res.push_str("[DRY RUN] ");
                }
                if *is_no_op {
                    res.push_str("[NO-OP] ");
                }
                res.push_str(format!("Deleting file: {}", path.display()).as_str());
                Some(res)
            }
        }
    }
}

pub fn pending_actions<'a>(actions: &'a Vec<Action>) -> Vec<&'a Action<'a>> {
    actions
        .iter()
        .filter(|action| match action {
            Action::Keep(_) => return false,
            Action::Symlink {
                is_no_op,
                path: _,
                source: _,
            } => !is_no_op,
            Action::Delete { is_no_op, path: _ } => !is_no_op,
        })
        .collect::<Vec<&Action>>()
}

pub fn execute(actions: Vec<Action>) -> Result<(), AppError> {
    for action in actions {
        let dry_run = true;
        if let Some(msg) = action.log(&dry_run) {
            info!("{}", msg);
        }
    }
    Ok(())
}
