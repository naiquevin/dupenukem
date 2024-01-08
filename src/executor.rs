use crate::error::AppError;
use crate::fileutil::{delete_file, replace_with_symlink};
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
    fn dry_run(&self) {
        match self {
            Self::Keep(_) => {}
            Self::Symlink {
                path,
                source,
                is_no_op,
            } => {
                let mut res = String::from("");
                res.push_str("[DRY RUN]");
                if *is_no_op {
                    res.push_str("[NO-OP]");
                }
                res.push_str(
                    format!(
                        " File to be replaced with symlink: {} -> {}",
                        path.display(),
                        // Here we're assuming that the source will never be
                        // None
                        source.display(),
                    )
                    .as_str(),
                );
                eprintln!("{}", res)
            }
            Self::Delete { path, is_no_op } => {
                let mut res = String::from("");
                res.push_str("[DRY RUN]");
                if *is_no_op {
                    res.push_str("[NO-OP]");
                }
                res.push_str(format!(" File to be deleted: {}", path.display()).as_str());
                eprintln!("{}", res)
            }
        }
    }

    #[allow(dead_code, unused)]
    fn execute(&self, backup_dir: Option<&PathBuf>, rootdir: &PathBuf) -> Result<(), AppError> {
        match self {
            Self::Keep(_) => Ok(()),
            Self::Symlink {
                path,
                source,
                is_no_op,
            } => {
                if !is_no_op {
                    info!(
                        "Replacing file with symlink: {} -> {}",
                        path.display(),
                        source.display()
                    );
                    replace_with_symlink(path, source, backup_dir, rootdir)
                } else {
                    info!(
                        "Intended symlink already exists (no-op): {} -> {}",
                        path.display(),
                        source.display()
                    );
                    Ok(())
                }
            }
            Self::Delete { path, is_no_op } => {
                if !is_no_op {
                    info!("Deleting file: {}", path.display());
                    delete_file(path, backup_dir, rootdir)
                } else {
                    info!("File already deleted: {}", path.display());
                    Ok(())
                }
            }
        }
    }
}

pub fn pending_actions<'a>(actions: &'a Vec<Action>, include_no_op: bool) -> Vec<&'a Action<'a>> {
    actions
        .iter()
        .filter(|action| match action {
            Action::Keep(_) => return false,
            Action::Symlink {
                is_no_op,
                path: _,
                source: _,
            } => include_no_op || !is_no_op,
            Action::Delete { is_no_op, path: _ } => include_no_op || !is_no_op,
        })
        .collect::<Vec<&Action>>()
}

pub fn execute(
    actions: Vec<Action>,
    dry_run: &bool,
    backup_dir: Option<&PathBuf>,
    rootdir: &PathBuf,
) -> Result<(), AppError> {
    // Here we're passing the `dry_run` arg as the 2nd arg so that if,
    //
    //  dry_run == true: no-op actions will be included and displayed
    //  dry_run == false: no-op actions will be skipped
    let actions_pending = pending_actions(&actions, *dry_run);
    info!(
        "Executing {} pending action(s) with dry_run={}",
        actions_pending.len(),
        dry_run
    );
    for action in actions_pending {
        if *dry_run {
            action.dry_run();
        } else {
            action.execute(backup_dir, rootdir)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_pending_actions() {
        let p1 = PathBuf::from("/a/1.txt");
        let p2 = PathBuf::from("/a/2.txt");
        let p3 = PathBuf::from("/a/3.txt");
        let p4 = PathBuf::from("/a/4.txt");
        let actions = vec![
            Action::Keep(&p1),
            Action::Symlink {
                path: &p2,
                source: &p3,
                is_no_op: true,
            },
            Action::Delete {
                path: &p4,
                is_no_op: false,
            },
        ];
        assert_eq!(2, pending_actions(&actions, true).len());
        assert_eq!(1, pending_actions(&actions, false).len());
    }
}
