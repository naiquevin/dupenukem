use crate::error::AppError;
use crate::snapshot::{textformat, Snapshot};
use clap::{self, Parser, Subcommand};
use inquire::Confirm;
use log::{debug, info};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process;

mod error;
mod executor;
mod fileutil;
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
        rootdir: PathBuf,
    },

    #[command(about = "Validate snapshot (from text representation)")]
    Validate {
        #[arg(long, help = "Read text from std input")]
        stdin: bool,
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
        snapshot_path: Option<PathBuf>,
    },
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

fn cmd_find(
    rootdir: &PathBuf,
    exclude: Option<&Vec<String>>,
    quick: &bool,
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
    let snap = Snapshot::of_rootdir(&rootdir, excludes.as_ref(), quick).map_err(AppError::Io)?;
    for line in textformat::render(&snap).iter() {
        println!("{}", line);
    }
    Ok(())
}

fn read_input(path: Option<&PathBuf>, stdin: &bool) -> Result<Vec<String>, AppError> {
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

fn cmd_validate(snapshot_path: Option<&PathBuf>, stdin: &bool) -> Result<(), AppError> {
    let input = read_input(snapshot_path, stdin)?;
    let snapshot = textformat::parse(input)?;
    match snapshot.validate() {
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

fn cmd_apply(
    snapshot_path: Option<&PathBuf>,
    stdin: &bool,
    dry_run: &bool,
) -> Result<(), AppError> {
    let input = read_input(snapshot_path, stdin)?;
    let snapshot = textformat::parse(input)?;
    let backup_dir = PathBuf::from("/tmp/dupenukem_backup");
    snapshot.validate().and_then(|actions| {
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
        executor::execute(actions, &dry_run, Some(&backup_dir), &snapshot.rootdir)
    })
}

impl Cli {
    fn execute(&self) -> Result<(), AppError> {
        match &self.command {
            Some(Command::Find {
                exclude,
                quick,
                rootdir,
            }) => cmd_find(rootdir, exclude.as_ref(), quick),
            Some(Command::Validate {
                stdin,
                snapshot_path,
            }) => cmd_validate(snapshot_path.as_ref(), stdin),
            Some(Command::Apply {
                stdin,
                snapshot_path,
                dry_run,
            }) => cmd_apply(snapshot_path.as_ref(), stdin, dry_run),
            None => Err(AppError::Cmd("Please specify the command".to_owned())),
        }
    }
}

fn main() {
    env_logger::init();
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
