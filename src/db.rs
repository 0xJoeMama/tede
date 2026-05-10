use std::{
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    num::{ParseFloatError, ParseIntError},
    path::{Path, PathBuf},
    str::FromStr,
};

use chrono::NaiveDate;
use tempfile::NamedTempFile;
use thiserror::Error;

pub const DATETIME_STR: &str = "%d-%m-%Y";

#[derive(Debug, Clone)]
pub struct TdeeEntry {
    pub calories: u32,
    pub weight: Option<f32>,
    pub date: NaiveDate,
}

impl TdeeEntry {
    pub fn new(calories: u32, weight: Option<f32>, date: NaiveDate) -> Self {
        Self {
            calories,
            weight,
            date,
        }
    }
}

#[derive(Error, Debug)]
pub enum ParseEntryError {
    #[error("no date was provided for entry")]
    NoDate,
    #[error("no calories were provided for entry")]
    NoCalories,
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("could not parse date: {0}")]
    DateError(#[from] chrono::format::ParseError),
    #[error("could not parse calories: {0}")]
    CaloriesError(#[from] ParseIntError),
    #[error("could not parse weight: {0}")]
    WeightError(#[from] ParseFloatError),
}

impl FromStr for TdeeEntry {
    type Err = ParseEntryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split(',').map(|it| it.trim());

        let date = iter.next().ok_or(ParseEntryError::NoDate).and_then(|it| {
            NaiveDate::parse_from_str(it.trim(), DATETIME_STR).map_err(|e| ParseEntryError::from(e))
        })?;

        let tdee = iter
            .next()
            .ok_or(ParseEntryError::NoCalories)
            .and_then(|it| {
                it.trim()
                    .parse::<u32>()
                    .map_err(|e| ParseEntryError::from(e))
            })?;

        let weight = iter
            .next()
            .map(|it| {
                it.trim()
                    .parse::<f32>()
                    .map_err(|e| ParseEntryError::from(e))
            })
            .transpose()?;

        Ok(Self {
            date,
            calories: tdee,
            weight,
        })
    }
}

impl Into<String> for TdeeEntry {
    fn into(self) -> String {
        format!(
            "{}, {}{}",
            self.date.format(DATETIME_STR),
            self.calories,
            self.weight
                .map(|it| format!(", {it}"))
                .unwrap_or_else(|| "".to_owned())
        )
    }
}

#[derive(Debug)]
pub struct TdeeBlock {
    pub initial_tdee: u32,
    pub entries: Vec<TdeeEntry>,
}

impl Into<String> for TdeeBlock {
    fn into(self) -> String {
        let entry_str = self
            .entries
            .into_iter()
            .map(|e| {
                let mut res: String = e.into();
                res.push('\n');
                res
            })
            .collect::<String>();

        format!("{}\n{}", self.initial_tdee, entry_str)
    }
}

impl TdeeBlock {
    pub fn iter(&self) -> impl Iterator<Item = &TdeeEntry> {
        self.entries.iter()
    }

    pub fn add_entry(&mut self, entry: TdeeEntry) {
        self.entries.push(entry)
    }
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("failed to open db file: {0}")]
    Io(#[from] std::io::Error),
    #[error("no initial tdee found")]
    NoInitial,
    #[error("could not parse initial tdee: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("could not parse entry line: {0}")]
    ParseEntry(#[from] ParseEntryError),
    #[error("block was empty")]
    EmptyBlock,
}

#[derive(Debug)]
pub struct TdeeDb {
    blocks: Vec<TdeeBlock>,
    path: PathBuf,
}

impl TdeeDb {
    pub fn from_file(path: &Path) -> Result<Self, DbError> {
        let file = File::options().append(true).read(true).open(path)?;

        // TODO: this currently just rewrite the whole file on every execution which is pretty bad
        let reader = BufReader::new(file);
        let mut lines = reader
            .lines()
            .map(|line| line.map_err(|e| DbError::from(e)));

        let mut blocks = vec![];
        while let Some(initial) = lines.next() {
            let initial = initial.and_then(|it| it.parse::<u32>().map_err(|e| DbError::from(e)))?;

            let mut records: Vec<TdeeEntry> = vec![];
            while let Some(record) = lines.next() {
                let record = record?;

                if record.trim().is_empty() {
                    break;
                }

                records.push(record.trim().parse()?);
            }

            if records.is_empty() {
                return Err(DbError::EmptyBlock);
            }

            blocks.push(TdeeBlock {
                initial_tdee: initial,
                entries: records,
            });
        }

        Ok(Self {
            blocks: blocks,
            path: path.to_path_buf(),
        })
    }

    pub fn new(path: &Path) -> Self {
        Self {
            blocks: vec![],
            path: path.to_path_buf(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &TdeeBlock> {
        self.blocks.iter()
    }

    pub fn commit(self) -> Result<(), DbError> {
        let file = NamedTempFile::new_in(
            self.path
                .parent()
                .expect("db is always a file and MUST always be in a directory"),
        )?;
        let path = file.path().to_path_buf();

        let mut writer = BufWriter::new(file);

        for block in self.blocks {
            writeln!(writer, "{}", <TdeeBlock as Into<String>>::into(block))?;
        }

        writer.flush()?;
        println!("renaming {} -> {}", path.display(), self.path.display());
        fs::rename(path, self.path)?;

        Ok(())
    }

    pub fn new_block(&mut self, initial_tdee: u32) -> &mut TdeeBlock {
        self.blocks.push(TdeeBlock {
            initial_tdee,
            entries: vec![],
        });

        self.blocks.last_mut().unwrap()
    }

    pub fn block(&mut self) -> Option<&mut TdeeBlock> {
        self.blocks.last_mut()
    }
}
