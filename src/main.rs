use crate::error::AppError;
use crate::snapshot::{textformat, Snapshot};
use clap::{self, Parser, Subcommand};
use log::info;
use std::path::PathBuf;
use std::process;

mod error;
mod fileutil;
mod snapshot;

#[derive(Subcommand)]
enum Command {
    #[command(about = "Find duplicates and generate a snapshot (text representation)")]
    Find { rootdir: PathBuf },
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

impl Cli {
    fn execute(&self) -> Result<(), AppError> {
        match &self.command {
            Some(Command::Find { rootdir }) => {
                info!("Generating snapshot for dir: {}", rootdir.display());
                let snap = Snapshot::of_rootdir(&rootdir).unwrap();
                for line in textformat::render(&snap).iter() {
                    println!("{}", line);
                }
                Ok(())
            }
            None => {
                eprintln!("Please specify the command");
                Err(AppError::Cmd)
            }
        }
    }
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();
    let result = cli.execute();
    match result {
        Ok(()) => process::exit(0),
        Err(e) => {
            eprintln!("Error: {:?}", e);
            process::exit(1);
        }
    }
}
