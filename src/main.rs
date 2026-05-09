use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};
use clap::*;
use tede::db::{DATETIME_STR, TdeeDb};

#[derive(Parser)]
#[command(version, about, author)]
struct Cli {
    #[command(subcommand)]
    subcmd: Option<Commands>,
    path: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Insert a new record to the database
    Add {
        /// calorie count for the day
        tdee: u32,
        /// when where they consumed
        date: Option<String>,
    },
    /// Create a new database instance
    Create { initial_tdee: u32 },
    /// Print all entries of the currently active database
    Print,
}

fn handle_subcmd(db_path: &Path, subcmd: Commands) {
    match subcmd {
        Commands::Create { initial_tdee } => {
            TdeeDb::create(db_path, initial_tdee).expect("ERROR: failed to create database file");
        }
        Commands::Add { tdee, date } => {
            let mut db = TdeeDb::open(db_path).expect("ERROR: failed to open database file");

            let date = date
                .map(|s| NaiveDate::parse_from_str(&s, DATETIME_STR))
                .transpose()
                .map(|opt| opt.unwrap_or(Local::now().naive_local().date()))
                .expect("ERROR: failed to parse provided date");

            db.add(tdee, date)
                .expect("ERROR: failed to commit database addition");
        }
        Commands::Print => {
            let db = TdeeDb::open(db_path).expect("ERROR: failed to open database file");

            for (i, entry) in db.entries().enumerate() {
                print!(
                    "Entry {}: Calories {}, recorded at {} ",
                    i,
                    entry.calories,
                    entry.date.format(DATETIME_STR).to_string(),
                );

                if let Some(weight) = entry.weight {
                    println!("(weight was {weight} kg)");
                } else {
                    println!("(no weight record)");
                }
            }
        }
    };
}

fn main() {
    let cli = Cli::parse();

    if let Some(subcmd) = cli.subcmd {
        handle_subcmd(&cli.path, subcmd);
    } else {
        todo!("interactive not implemented");
    }
}
