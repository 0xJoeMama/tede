use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};
use clap::*;
use tede::db::{DATETIME_STR, TdeeDb, TdeeEntry};

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
        /// Weight for the target date
        weight: Option<f32>,
        #[arg(short, long)]
        /// when where they consumed
        date: Option<String>,
        /// specify whether or not the file should be created if it does not exist
        #[arg(short, long, default_value_t = false)]
        create: bool,
    },
    /// Print all entries of the currently active database
    Print,
}

impl Commands {
    fn run(self, db_path: &Path) {
        match self {
            Commands::Add {
                tdee,
                date,
                create,
                weight,
            } => {
                let date = date
                    .map(|s| NaiveDate::parse_from_str(&s, DATETIME_STR))
                    .transpose()
                    .map(|opt| opt.unwrap_or(Local::now().naive_local().date()))
                    .expect("ERROR: failed to parse provided date");

                if create {
                    let mut db = TdeeDb::new(db_path);
                    let block = db.new_block(tdee);
                    block.add_entry(TdeeEntry::new(tdee, weight, date));

                    if let Err(e) = db.commit() {
                        panic!("ERROR: failed to commit changes to local database: {e}");
                    }

                    return;
                }

                let mut db = match TdeeDb::from_file(db_path) {
                    Ok(db) => db,
                    Err(e) => panic!("ERROR: failed to open database file: {}", e),
                };

                if let Some(block) = db.block() {
                    block.add_entry(TdeeEntry::new(tdee, weight, date));
                }

                if let Err(e) = db.commit() {
                    panic!("ERROR: failed to commit changes to local database: {e}");
                }
            }
            Commands::Print => {
                let db = match TdeeDb::from_file(db_path) {
                    Ok(db) => db,
                    Err(e) => panic!("ERROR: failed to open database file: {}", e),
                };

                for (i, block) in db.iter().enumerate() {
                    println!("Block {} has {} entries", i, block.entries.len());

                    for (j, entry) in block.iter().enumerate() {
                        print!(
                            "Entry {j}: calories {}, date is {} ",
                            entry.calories,
                            entry.date.format(DATETIME_STR)
                        );
                        if let Some(weight) = entry.weight {
                            println!("(weight was {weight} kg)");
                        } else {
                            println!("(no weight record)");
                        }
                    }
                }
            }
        };
    }
}

fn main() {
    let cli = Cli::parse();

    if let Some(subcmd) = cli.subcmd {
        subcmd.run(&cli.path);
    } else {
        todo!("interactive not implemented");
    }
}
