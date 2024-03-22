use crate::error::AppError;
use crate::snapshot::{textformat, Snapshot};
use chrono::offset::Local;
use clap::{self, Parser, Subcommand};
use dirs::home_dir;
use inquire::Confirm;
use log::{debug, info};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process;

mod error;
mod executor;
mod fileutil;
mod hash;
mod ioutil;
mod scanner;
mod snapshot;

#[derive(Subcommand)]
enum Command {
    #[command(about = "Find duplicates and generate a snapshot (text representation)")]
    Find {
        #[arg(long, help = "Exclude (relative) paths")]
        exclude: Option<Vec<String>>,
        #[arg(
            long,
            default_value_t = false,
            help = "Quick mode in which sha256 comparison is skipped and only xxhash3(64) hashes are compared instead"
        )]
        quick: bool,
        #[arg(long, help = "Donot list symlinks in snapshot output")]
        skip_deduped: bool,
        rootdir: PathBuf,
    },

    #[command(about = "Validate snapshot (from text representation)")]
    Validate {
        #[arg(long, help = "Read text from std input")]
        stdin: bool,
        #[arg(long, help = "Allow deletion of all files in a group")]
        allow_full_deletion: bool,
        snapshot_path: Option<PathBuf>,
    },

    #[command(about = "Apply changes from snapshot file")]
    Apply {
        #[arg(long, help = "Read text from std input")]
        stdin: bool,
        #[arg(
            long,
            help = "Dry run i.e. the actions will only be logged and not actually run"
        )]
        dry_run: bool,
        #[arg(long, help = "Allow deletion of all files in a group")]
        allow_full_deletion: bool,
        #[arg(
            long,
            help = "Custom backup directory. If not specified, a default one based on current timestamp will be used"
        )]
        backup_dir: Option<PathBuf>,
        snapshot_path: Option<PathBuf>,
    },
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, global = true, action = clap::ArgAction::Count, help = "Verbosity level (can be specified multiple times)")]
    verbose: u8,
    #[command(subcommand)]
    command: Option<Command>,
}

fn cmd_find(
    rootdir: &Path,
    exclude: Option<&Vec<String>>,
    quick: &bool,
    skip_deduped: &bool,
) -> Result<(), AppError> {
    let rootdir = if !rootdir.is_absolute() {
        info!("Relative path found for the specified rootdir. Normalizing it to absolute path");
        rootdir.canonicalize().map_err(AppError::Io)?
    } else {
        // @NOTE: How to avoid creating a copy here?
        rootdir.to_path_buf()
    };
    let excludes = exclude.map(|paths| HashSet::from_iter(paths.iter().map(|p| rootdir.join(p))));
    info!("Generating snapshot for dir: {}", rootdir.display());
    if let Some(exs) = &excludes {
        info!(
            "Exclusions: {}",
            exs.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );
    }
    let snap = Snapshot::of_rootdir(&rootdir, excludes.as_ref(), quick, skip_deduped)
        .map_err(AppError::Io)?;
    snap.freeable_space()
        .map(|total| info!("A max of {} space can be freed by deduplication", total))
        .map_err(AppError::Io)?;
    let output = textformat::render(&snap);
    if !output.is_empty() {
        for line in output.iter() {
            println!("{}", line);
        }
    } else {
        eprintln!("No duplicates found under path: {}", rootdir.display());
    }
    Ok(())
}

fn read_input(path: Option<&Path>, stdin: &bool) -> Result<Vec<String>, AppError> {
    match path {
        Some(p) => ioutil::read_lines_in_file(p).map_err(AppError::Io),
        None => {
            if *stdin {
                ioutil::stdin_to_vec().map_err(AppError::Io)
            } else {
                Err(AppError::Cmd(
                    "Either snapshot filepath or '--stdin' option must be specified".to_owned(),
                ))
            }
        }
    }
}

fn cmd_validate(
    snapshot_path: Option<&Path>,
    stdin: &bool,
    allow_full_deletion: &bool,
) -> Result<(), AppError> {
    let input = read_input(snapshot_path, stdin)?;
    let snapshot = textformat::parse(input)?;
    match snapshot.validate(allow_full_deletion) {
        Ok(actions) => {
            println!("Snapshot is valid!");
            let num_pending = executor::pending_actions(&actions, false).len();
            if num_pending == 0 {
                println!("No pending actions");
            } else {
                println!("No. of pending action(s): {}", num_pending);
            }
            Ok(())
        }
        Err(e) => {
            println!("Snapshot is invalid!");
            Err(e)
        }
    }
}

/// Returns default backup dir derived from the current timestamp.
///
/// The path prefix will be `~/.dupenukem/backups` if home dir can be
/// obtained for the user otherwise it will be under the `$CWD`
/// i.e. `./.dupenukem/backups`
///
/// Example backup dir path: `~/.dupenukem/backups/20240109163803`
///
fn default_backup_dir() -> PathBuf {
    let path_prefix = home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dupenukem/backups");
    let dirname = Local::now().format("%Y%m%d%H%M%S");
    path_prefix.join(dirname.to_string())
}

fn cmd_apply(
    snapshot_path: Option<&Path>,
    stdin: &bool,
    dry_run: &bool,
    allow_full_deletion: &bool,
    backup_dir: &Option<PathBuf>,
) -> Result<(), AppError> {
    let input = read_input(snapshot_path, stdin)?;
    let snapshot = textformat::parse(input)?;
    // A tmp let binding for default backup dir is required here
    // because the fallback value in `unwrap_or` is a pointer and not
    // a value.
    let dbd = default_backup_dir();
    let backup_dir_path = backup_dir.as_ref().unwrap_or(&dbd);
    snapshot.validate(allow_full_deletion).and_then(|actions| {
        if !*dry_run {
            let ans = Confirm::new("All changes will be executed. Do you want to proceed?")
                .with_default(false)
                .with_help_message(
                    "Tip: To see the changes run the command with '--dry-run' option",
                )
                .prompt();
            match ans {
                Ok(true) => debug!("Received confirmation from user. Proceeding.."),
                Ok(false) => {
                    debug!("User asked to abort");
                    println!("Aborting..");
                    process::exit(0);
                }
                Err(e) => {
                    debug!("Error encountered in confirm prompt: {:?}", e);
                    println!("Something went wrong. Aborting..");
                    process::exit(1);
                }
            }
        }
        executor::execute(actions, dry_run, Some(backup_dir_path), &snapshot.rootdir)
    })
}

fn init_logging(verbosity: u8) {
    let log_level = match verbosity {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let env = env_logger::Env::default().default_filter_or(log_level);
    env_logger::Builder::from_env(env).init()
}

impl Cli {
    fn execute(&self) -> Result<(), AppError> {
        init_logging(self.verbose);
        match &self.command {
            Some(Command::Find {
                exclude,
                quick,
                skip_deduped,
                rootdir,
            }) => cmd_find(rootdir, exclude.as_ref(), quick, skip_deduped),
            Some(Command::Validate {
                stdin,
                allow_full_deletion,
                snapshot_path,
            }) => cmd_validate(
                snapshot_path.as_ref().map(|p| p.as_ref()),
                stdin,
                allow_full_deletion,
            ),
            Some(Command::Apply {
                stdin,
                snapshot_path,
                dry_run,
                allow_full_deletion,
                backup_dir,
            }) => cmd_apply(
                snapshot_path.as_ref().map(|p| p.as_ref()),
                stdin,
                dry_run,
                allow_full_deletion,
                backup_dir,
            ),
            None => Err(AppError::Cmd("Please specify the command".to_owned())),
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let result = cli.execute();
    match result {
        Ok(()) => process::exit(0),
        Err(AppError::Cmd(msg)) => {
            eprintln!("Command Error: {}", msg);
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
            process::exit(1);
        }
    }
}
