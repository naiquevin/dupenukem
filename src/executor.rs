use crate::error::AppError;
use crate::fileutil::{
    delete_file, normalize_path, normalize_symlink_src_path, replace_with_symlink,
};
use log::info;
use std::path::Path;

#[derive(Debug)]
pub enum Action<'a> {
    Keep(&'a Path),
    Symlink {
        path: &'a Path,
        source: &'a Path,
        is_explicit: bool,
        is_no_op: bool,
    },
    Delete {
        path: &'a Path,
        is_no_op: bool,
    },
}

impl<'a> Action<'a> {
    fn dry_run(&self, rootdir: &Path) {
        match self {
            Self::Keep(_) => {}
            Self::Symlink {
                path,
                source,
                is_explicit,
                is_no_op,
            } => {
                let mut res = String::from("");
                res.push_str("[DRY RUN]");
                if *is_no_op {
                    res.push_str("[NO-OP]");
                }

                let src_path = normalize_symlink_src_path(path, source, *is_explicit).unwrap();

                // Use relative path in dry-run output
                let rel_path = normalize_path(path, true, rootdir).unwrap();
                res.push_str(
                    format!(
                        " File to be replaced with symlink: {} -> {}",
                        rel_path.display(),
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

    fn execute(&self, backup_dir: Option<&Path>, rootdir: &Path) -> Result<(), AppError> {
        match self {
            Self::Keep(_) => Ok(()),
            Self::Symlink {
                path,
                source,
                is_explicit,
                is_no_op,
            } => {
                let src_path = normalize_symlink_src_path(path, source, *is_explicit).unwrap();

                // Show relative path in log messages
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
                // Show relative path in log messages
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

pub fn pending_actions<'a>(actions: &'a [Action], include_no_op: bool) -> Vec<&'a Action<'a>> {
    actions
        .iter()
        .filter(|action| match action {
            Action::Keep(_) => false,
            Action::Symlink {
                is_no_op,
                path: _,
                source: _,
                is_explicit: _,
            } => include_no_op || !is_no_op,
            Action::Delete { is_no_op, path: _ } => include_no_op || !is_no_op,
        })
        .collect::<Vec<&Action>>()
}

pub fn execute(
    actions: Vec<Action>,
    dry_run: &bool,
    backup_dir: Option<&Path>,
    rootdir: &Path,
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
    if *dry_run {
        match backup_dir {
            Some(d) => eprintln!(
                "[DRY RUN] Backup will be stored under {}",
                d.parent().unwrap().display()
            ),
            None => eprintln!("[DRY RUN] Backup is disabled (not recommended)"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_pending_actions() {
        let p1 = Path::new("/a/1.txt");
        let p2 = Path::new("/a/2.txt");
        let p3 = Path::new("/a/3.txt");
        let p4 = Path::new("/a/4.txt");
        let actions = vec![
            Action::Keep(&p1),
            Action::Symlink {
                path: &p2,
                source: &p3,
                is_no_op: true,
                is_explicit: true,
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
