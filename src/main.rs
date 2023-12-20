use crate::error::AppError;
use crate::snapshot::{textformat, Snapshot};
use clap::{self, Parser, Subcommand};
use log::info;
use std::path::PathBuf;
use std::process;

mod error;
mod fileutil;
mod ioutil;
mod snapshot;

#[derive(Subcommand)]
enum Command {
    #[command(about = "Find duplicates and generate a snapshot (text representation)")]
    Find { rootdir: PathBuf },

    #[command(about = "Validate snapshot (from text representation)")]
    Validate {
        #[arg(long, help = "Read text from std input")]
        stdin: bool,
        snapshot_path: Option<PathBuf>,
    },
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

fn cmd_find(rootdir: &PathBuf) -> Result<(), AppError> {
    info!("Generating snapshot for dir: {}", rootdir.display());
    let snap = Snapshot::of_rootdir(rootdir).unwrap();
    for line in textformat::render(&snap).iter() {
        println!("{}", line);
    }
    Ok(())
}

fn cmd_validate(snapshot_path: &Option<PathBuf>, stdin: &bool) -> Result<(), AppError> {
    let input = match snapshot_path {
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
    }?;
    let snapshot = textformat::parse(input)?;
    match snapshot.validate() {
        Ok(actions) => {
            for action in actions {
                println!("{:?}", action);
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

impl Cli {
    fn execute(&self) -> Result<(), AppError> {
        match &self.command {
            Some(Command::Find { rootdir }) => cmd_find(rootdir),
            Some(Command::Validate {
                stdin,
                snapshot_path,
            }) => cmd_validate(snapshot_path, stdin),
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
