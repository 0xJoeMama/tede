use std::{
    cmp::Ordering,
    collections::BTreeMap,
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct TdeeEntry {
    pub calories: i32,
    pub weight: Option<f32>,
    pub date: NaiveDate,
}

impl TdeeEntry {
    pub fn new(calories: i32, weight: Option<f32>, date: NaiveDate) -> Self {
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
            NaiveDate::parse_from_str(it.trim(), DATETIME_STR).map_err(ParseEntryError::from)
        })?;

        let calories = iter
            .next()
            .ok_or(ParseEntryError::NoCalories)
            .and_then(|it| it.trim().parse::<i32>().map_err(ParseEntryError::from))?;

        let weight = iter
            .next()
            .map(|it| it.trim().parse::<f32>().map_err(ParseEntryError::from))
            .transpose()?;

        Ok(Self {
            date,
            calories,
            weight,
        })
    }
}

impl From<TdeeEntry> for String {
    fn from(val: TdeeEntry) -> Self {
        format!(
            "{}, {}{}",
            val.date.format(DATETIME_STR),
            val.calories,
            val.weight
                .map(|it| format!(", {it}"))
                .unwrap_or_else(|| "".to_owned())
        )
    }
}

#[derive(Debug)]
pub struct TdeeBlock {
    pub initial_tdee: i32,
    entries: BTreeMap<NaiveDate, TdeeEntry>,
}

impl From<TdeeBlock> for String {
    fn from(val: TdeeBlock) -> Self {
        let entry_str = val
            .entries
            .into_iter()
            .map(|(_, e)| {
                let mut res: String = e.into();
                res.push('\n');
                res
            })
            .collect::<String>();

        format!("{}\n{}", val.initial_tdee, entry_str)
    }
}

impl TdeeBlock {
    pub fn new(initial_tdee: i32) -> Self {
        Self {
            initial_tdee,
            entries: BTreeMap::new(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &TdeeEntry> {
        self.entries.iter().map(|(_, it)| it)
    }

    pub fn weight_entry_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|it| it.1.weight.is_some())
            .count()
    }

    pub fn add_entry(&mut self, entry: TdeeEntry) -> Result<(), DbError> {
        if self.entries.contains_key(&entry.date) {
            return Err(DbError::DuplicateEntry(entry.date));
        }

        self.entries.insert(entry.date, entry);
        Ok(())
    }

    pub fn get_weight_rate(&self) -> f32 {
        let iter = self.iter().filter_map(|it| it.weight).collect::<Vec<_>>();

        // TODO: change leniency window
        const LENIENCY: usize = 4;
        assert!(iter.len() > 4);

        let first_week = iter.iter().skip(LENIENCY).take(7).sum::<f32>() / 7.0;
        let last_week = iter.iter().rev().take(7).sum::<f32>() / 7.0;

        let week_cnt = (iter.len() - LENIENCY) as f32 / 7.0;

        return (last_week - first_week) / week_cnt;
    }

    pub fn get_average_calories(&self) -> i32 {
        self.entries.iter().map(|(_, it)| it.calories).sum::<i32>() / (self.entries.len() as i32)
    }

    pub fn entry_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|it| it.1.weight.is_some())
            .count()
    }
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("failed to open db file: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not parse integer: {0}")]
    ParseInt(#[from] ParseIntError),
    #[error("could not parse float: {0}")]
    ParseFloat(#[from] ParseFloatError),
    #[error("could not parse entry line: {0}")]
    ParseEntry(#[from] ParseEntryError),
    #[error("block was empty")]
    EmptyBlock,
    #[error("missing database metadata field: {0}")]
    MissingField(String),
    #[error("entry with date {0} already exists")]
    DuplicateEntry(NaiveDate),
    #[error("expected a new line")]
    ExpectNewline,
}

#[derive(Debug)]
pub struct DbHeader {
    starting_weight: f32,
    target_weight: f32,
    rate: f32,
    step: i32,
    grace_period: usize,
}

impl DbHeader {
    pub fn from_file(
        lines: &mut impl Iterator<Item = Result<String, DbError>>,
    ) -> Result<Self, DbError> {
        let starting_weight = lines
            .next()
            .ok_or(DbError::MissingField("starting weight".to_owned()))??
            .trim()
            .parse()?;

        let target_weight = lines
            .next()
            .ok_or(DbError::MissingField("target weight".to_owned()))??
            .trim()
            .parse()?;

        let rate = lines
            .next()
            .ok_or(DbError::MissingField("rate".to_owned()))??
            .trim()
            .parse()?;

        let increment = lines
            .next()
            .ok_or(DbError::MissingField("increment".to_owned()))??
            .trim()
            .parse()?;

        let grace_period = lines
            .next()
            .ok_or(DbError::MissingField("grace period".to_owned()))??
            .trim()
            .parse()?;

        Ok(Self {
            starting_weight,
            target_weight,
            rate,
            step: increment,
            grace_period,
        })
    }

    pub fn commit(&self, writer: &mut impl Write) -> Result<(), DbError> {
        writeln!(writer, "{}", self.starting_weight)?;
        writeln!(writer, "{}", self.target_weight)?;
        writeln!(writer, "{}", self.rate)?;
        writeln!(writer, "{}", self.step)?;
        writeln!(writer, "{}", self.grace_period)?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct TdeeDb {
    header: DbHeader,
    blocks: Vec<TdeeBlock>,
    path: PathBuf,
}

impl TdeeDb {
    pub fn from_file(path: &Path) -> Result<Self, DbError> {
        let file = File::options().append(true).read(true).open(path)?;

        // TODO: this currently just rewrites the whole file on every execution which is pretty bad
        let reader = BufReader::new(file);
        let mut lines = reader.lines().map(|line| line.map_err(DbError::from));

        let header = DbHeader::from_file(&mut lines)?;

        if !lines
            .next()
            .ok_or(DbError::ExpectNewline)??
            .trim()
            .is_empty()
        {
            return Err(DbError::ExpectNewline);
        }

        let mut blocks = vec![];
        while let Some(initial) = lines.next() {
            let initial = initial.and_then(|it| it.parse::<i32>().map_err(DbError::from))?;

            let mut records = BTreeMap::new();
            for record in lines.by_ref() {
                let record = record?;

                if record.trim().is_empty() {
                    break;
                }

                let record: TdeeEntry = record.trim().parse()?;
                records.insert(record.date, record);
            }

            blocks.push(TdeeBlock {
                initial_tdee: initial,
                entries: records,
            });
        }

        Ok(Self {
            header,
            blocks,
            path: path.to_path_buf(),
        })
    }

    /// Create a db with the default grace period and a basic increment
    pub fn new(
        path: &Path,
        starting_weight: f32,
        target_weight: f32,
        rate: f32,
        increment: i32,
        grace_period: usize,
        initial_tdee: i32,
    ) -> Self {
        Self {
            header: DbHeader {
                starting_weight,
                target_weight,
                rate,
                step: increment,
                grace_period,
            },
            // insert the first block. First block is always empty
            blocks: vec![TdeeBlock::new(initial_tdee)],
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

        self.header.commit(&mut writer)?;

        writeln!(writer)?;

        for block in self.blocks {
            writeln!(writer, "{}", String::from(block))?;
        }

        writer.flush()?;
        println!("renaming {} -> {}", path.display(), self.path.display());
        fs::rename(path, self.path)?;

        Ok(())
    }

    fn calculate_new_tdee(&self, block: &TdeeBlock) -> i32 {
        let weight_diff = block.get_weight_rate();
        let avg_calories = block.get_average_calories();
        let delta = self.header.target_weight - self.header.starting_weight;
        let sign = delta.signum() as i32;

        let new_calories = match weight_diff
            .partial_cmp(&self.header.rate)
            .expect("could not make float comparison")
        {
            Ordering::Less => avg_calories + sign * self.header.step,
            Ordering::Equal => avg_calories,
            Ordering::Greater => avg_calories - sign * self.header.step,
        };

        new_calories
    }

    pub fn new_block(&mut self) -> &mut TdeeBlock {
        let last = self
            .blocks
            .last()
            .expect("database file must always have at least one block");

        // initial TDEE of new block is the adjusted TDEE of the last block
        let initial_tdee = self.calculate_new_tdee(last);

        self.blocks.push(TdeeBlock {
            initial_tdee,
            entries: BTreeMap::new(),
        });

        self.blocks.last_mut().unwrap()
    }

    pub fn add_entry(&mut self, entry: TdeeEntry) -> Result<(), DbError> {
        if self
            .blocks
            .last()
            .expect("db must have at least one block")
            .entries
            .len()
            < self.header.grace_period
        {
            self.blocks
                .last_mut()
                .expect("db must have at least one block")
                .add_entry(entry)?;
            Ok(())
        } else {
            self.new_block().add_entry(entry)
        }
    }
}
