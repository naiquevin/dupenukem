use crate::error::AppError;
use crate::fileutil::{delete_file, normalize_path, replace_with_symlink};
use log::info;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Action<'a> {
    Keep(&'a PathBuf),
    Symlink {
        path: &'a PathBuf,
        source: &'a PathBuf,
        is_relative: bool,
        is_no_op: bool,
    },
    Delete {
        path: &'a PathBuf,
        is_no_op: bool,
    },
}

impl<'a> Action<'a> {
    fn dry_run(&self, rootdir: &PathBuf) {
        match self {
            Self::Keep(_) => {}
            Self::Symlink {
                path,
                source,
                is_relative,
                is_no_op,
            } => {
                let mut res = String::from("");
                res.push_str("[DRY RUN]");
                if *is_no_op {
                    res.push_str("[NO-OP]");
                }
                let src_path = normalize_path(source, *is_relative, rootdir).unwrap();
                // Use relative path in dry-run output
                let rel_path = normalize_path(path, true, rootdir).unwrap();
                res.push_str(
                    format!(
                        " File to be replaced with symlink: {} -> {}",
                        rel_path.display(),
                        // Here we're assuming that the source will never be
                        // None
                        src_path.display(),
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
                // Use relative path in dry-run output
                let rel_path = normalize_path(path, true, rootdir).unwrap();
                res.push_str(format!(" File to be deleted: {}", rel_path.display()).as_str());
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
                is_relative,
                is_no_op,
            } => {
                let src_path = normalize_path(source, *is_relative, rootdir)?;
                // Use relative path in log messages
                let rel_path = normalize_path(path, true, rootdir).unwrap();
                if !is_no_op {
                    info!(
                        "Replacing file with symlink: {} -> {}",
                        rel_path.display(),
                        src_path.display()
                    );
                    replace_with_symlink(path, &src_path, backup_dir, rootdir)
                } else {
                    info!(
                        "Intended symlink already exists (no-op): {} -> {}",
                        rel_path.display(),
                        src_path.display()
                    );
                    Ok(())
                }
            }
            Self::Delete { path, is_no_op } => {
                // Use relative path in log messages
                let rel_path = normalize_path(path, true, rootdir).unwrap();
                if !is_no_op {
                    info!("Deleting file: {}", rel_path.display());
                    delete_file(path, backup_dir, rootdir)
                } else {
                    info!("File already deleted: {}", rel_path.display());
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
                is_relative: _,
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
            action.dry_run(rootdir);
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
                is_relative: true,
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
