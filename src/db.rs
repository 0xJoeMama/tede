use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    num::{ParseFloatError, ParseIntError},
    path::Path,
    str::FromStr,
};

use chrono::NaiveDate;
use thiserror::Error;

pub const DATETIME_STR: &str = "%d-%m-%Y";

#[derive(Debug)]
pub struct TdeeEntry {
    pub calories: u32,
    pub weight: Option<f32>,
    pub date: NaiveDate,
}

#[derive(Error, Debug)]
pub enum ParseEntryError {
    #[error("no date was provided for entry")]
    NoDate,
    #[error("no calories were provided for entry")]
    NoCalories,
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("could not parse date")]
    DateError(#[from] chrono::format::ParseError),
    #[error("could not parse calories")]
    CaloriesError(#[from] ParseIntError),
    #[error("could not parse weight")]
    WeightError(#[from] ParseFloatError),
}

impl FromStr for TdeeEntry {
    type Err = ParseEntryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split(',').map(|it| it.trim());

        let date = iter.next().ok_or(ParseEntryError::NoDate).and_then(|it| {
            NaiveDate::parse_from_str(it, DATETIME_STR).map_err(|e| ParseEntryError::from(e))
        })?;

        let tdee = iter
            .next()
            .ok_or(ParseEntryError::NoCalories)
            .and_then(|it| it.parse::<u32>().map_err(|e| ParseEntryError::from(e)))?;

        let weight = iter
            .next()
            .map(|it| it.parse::<f32>().map_err(|e| ParseEntryError::from(e)))
            .transpose()?;

        Ok(Self {
            date,
            calories: tdee,
            weight,
        })
    }
}

#[derive(Debug)]
pub struct TdeeDb {
    initial_tdee: u32,
    entries: Vec<TdeeEntry>,
    file: File,
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("failed to open db file")]
    Io(#[from] std::io::Error),
    #[error("no initial tdee found")]
    NoInitial,
    #[error("could not parse initial tdee")]
    ParseInt(#[from] ParseIntError),
    #[error("could not parse entry line")]
    ParseEntry(#[from] ParseEntryError),
}

impl TdeeDb {
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let file = File::options().append(true).read(true).open(path)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let initial = lines
            .next()
            .ok_or(DbError::NoInitial)?
            .map_err(|e| DbError::from(e))
            .and_then(|it| it.parse::<u32>().map_err(|e| DbError::from(e)))?;

        let entries = lines
            .map(|it| {
                it.map_err(|e| DbError::from(e))
                    .and_then(|it| Ok(it.parse::<TdeeEntry>()?))
            })
            .collect::<Result<Vec<_>, DbError>>()?;

        Ok(Self {
            initial_tdee: initial,
            entries,
            file: File::options().append(true).read(true).open(path)?,
        })
    }

    pub fn create(path: &Path, initial: u32) -> Result<(), DbError> {
        let mut file = File::create_new(path)?;
        writeln!(file, "{initial}")?;
        Ok(())
    }

    pub fn add(&mut self, tdee: u32, date: NaiveDate) -> Result<(), DbError> {
        Ok(writeln!(
            self.file,
            "{}, {}",
            date.format(DATETIME_STR).to_string(),
            tdee
        )?)
    }

    pub fn entries(&self) -> impl Iterator<Item = &TdeeEntry> {
        self.entries.iter()
    }
}
