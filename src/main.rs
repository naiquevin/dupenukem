use crate::error::AppError;
use crate::snapshot::{textformat, Snapshot};
use clap::{self, Parser, Subcommand};
use log::info;
use std::io;
use std::path::PathBuf;
use std::process;

mod error;
mod fileutil;
mod snapshot;

pub fn stdin_to_vec() -> Vec<String> {
    let stdin = io::stdin();
    let mut result = Vec::new();
    for line in stdin.lines() {
        let s = line.unwrap();
        result.push(s);
    }
    result
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Find duplicates and generate a snapshot (text representation)")]
    Find { rootdir: PathBuf },

    #[command(about = "Validate snapshot (from text representation)")]
    Validate {
        #[arg(long, help = "Read text from std input")]
        stdin: bool,
    },
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
            Some(Command::Validate { stdin }) => {
                if *stdin {
                    let input = stdin_to_vec();
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
                } else {
                    log::error!("File input not supported. Please use --stdin for now");
                    Err(AppError::Cmd)
                }
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
