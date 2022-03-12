use error::Result;
use lazy_static::lazy_static;
use log::debug;
use regex::Regex;

pub mod error;

pub static DUMP_INDEX_URL: &'static str = "https://dumps.wikimedia.org/backup-index.html";

lazy_static! {
    static ref LIST_WIKTIONARY_DUMP_LANGUAGES_REGEX: Regex =
        Regex::new(r#"<a href="([a-z\-]{2,20})wiktionary/[0-9]{8}">"#).unwrap();
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DumpIndexUrl {
    Default,
    Custom(String),
}

impl DumpIndexUrl {
    pub fn as_str(&self) -> &str {
        match self {
            DumpIndexUrl::Default => DUMP_INDEX_URL,
            DumpIndexUrl::Custom(custom) => custom,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LanguageCode {
    English,
    French,
    Russian,
    German,
    Finnish,
    Unknown(String),
}

impl<'a> From<&'a str> for LanguageCode {
    fn from(string: &'a str) -> Self {
        match string {
            "en" => Self::English,
            "fr" => Self::French,
            "ru" => Self::Russian,
            "de" => Self::German,
            "fi" => Self::Finnish,
            string => Self::Unknown(string.to_string()), }
    }
}

pub async fn list_wiktionary_dump_languages(url: &DumpIndexUrl) -> Result<Vec<LanguageCode>> {
    let body = reqwest::get(url.as_str()).await?.text().await?;
    debug!(
        "language_regex: {:?}",
        *LIST_WIKTIONARY_DUMP_LANGUAGES_REGEX
    );
    Ok(LIST_WIKTIONARY_DUMP_LANGUAGES_REGEX
        .captures_iter(&body)
        .map(|captures| captures[1].into())
        .collect())
}
