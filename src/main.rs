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
        tdee: i32,
        /// Weight for the target date
        weight: Option<f32>,
        #[arg(short, long)]
        /// when where they consumed
        date: Option<String>,
    },
    /// Create a new database file
    Create {
        starting_weight: f32,
        target_weight: f32,
        rate: f32,
        #[arg(short, long, default_value_t = 200)]
        step: i32,
        #[arg(short, long, default_value_t = 21)]
        grace_period: usize,
        initial_calories: i32,
    },
    /// Print all entries of the currently active database
    Print,
}

impl Commands {
    fn run(self, db_path: &Path) {
        match self {
            Commands::Add { tdee, date, weight } => {
                let date = date
                    .map(|s| NaiveDate::parse_from_str(&s, DATETIME_STR))
                    .transpose()
                    .map(|opt| opt.unwrap_or(Local::now().naive_local().date()))
                    .expect("ERROR: failed to parse provided date");

                let mut db = match TdeeDb::from_file(db_path) {
                    Ok(db) => db,
                    Err(e) => panic!("ERROR: failed to open database file: {}", e),
                };

                if let Err(e) = db.add_entry(TdeeEntry::new(tdee, weight, date)) {
                    panic!("ERROR: failed to add new entry to database: {e}");
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
                    println!("Block {} has {} entries", i, block.iter().count());

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
            Commands::Create {
                starting_weight,
                target_weight,
                rate,
                step,
                grace_period,
                initial_calories,
            } => {
                let db = TdeeDb::new(
                    db_path,
                    starting_weight,
                    target_weight,
                    rate,
                    step,
                    grace_period,
                    initial_calories,
                );

                if let Err(e) = db.commit() {
                    panic!("ERROR: failed to commit new database: {e}");
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
