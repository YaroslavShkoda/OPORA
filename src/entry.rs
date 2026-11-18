//! Одна запись дня. Дата — единственное, что связывает запись с реальностью.

use anyhow::{Result, bail};
use chrono::NaiveDate;

pub const DATE_FMT: &str = "%Y-%m-%d";

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub date: NaiveDate,
    /// Может отсутствовать: «не помню, сколько спал».
    pub sleep_hours: Option<f32>,
    /// 1..=10
    pub mood: u8,
    pub note: String,
}

impl Entry {
    pub fn new(
        date: NaiveDate,
        sleep_hours: Option<f32>,
        mood: u8,
        note: impl Into<String>,
    ) -> Result<Self> {
        let entry = Self {
            date,
            sleep_hours,
            mood,
            note: note.into(),
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn validate(&self) -> Result<()> {
        if !(1..=10).contains(&self.mood) {
            bail!("настроение должно быть от 1 до 10, получено {}", self.mood);
        }
        if let Some(hours) = self.sleep_hours
            && !(0.0..=24.0).contains(&hours)
        {
            bail!("сон должен быть от 0 до 24 часов, получено {hours}");
        }
        Ok(())
    }

    pub fn date_str(&self) -> String {
        self.date.format(DATE_FMT).to_string()
    }
}

pub fn parse_date(text: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), DATE_FMT)
        .map_err(|e| anyhow::anyhow!("дата должна быть в формате ГГГГ-ММ-ДД: {e}"))
}

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 18).unwrap()
    }

    #[test]
    fn mood_bounds() {
        assert!(Entry::new(date(), Some(8.0), 1, "").is_ok());
        assert!(Entry::new(date(), Some(8.0), 10, "").is_ok());
        assert!(Entry::new(date(), Some(8.0), 0, "").is_err());
        assert!(Entry::new(date(), Some(8.0), 11, "").is_err());
    }

    #[test]
    fn sleep_bounds() {
        assert!(Entry::new(date(), None, 5, "").is_ok());
        assert!(Entry::new(date(), Some(0.0), 5, "").is_ok());
        assert!(Entry::new(date(), Some(24.0), 5, "").is_ok());
        assert!(Entry::new(date(), Some(24.5), 5, "").is_err());
        assert!(Entry::new(date(), Some(-1.0), 5, "").is_err());
    }

    #[test]
    fn date_roundtrip() {
        let entry = Entry::new(date(), None, 5, "").unwrap();
        assert_eq!(entry.date_str(), "2026-11-18");
        assert_eq!(parse_date("2026-11-18").unwrap(), date());
        assert!(parse_date("18.11.2026").is_err());
    }
}
