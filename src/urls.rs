use url::Url;
use crate::error::Result;
use crate::language_code::LanguageCode;

static DUMP_INDEX_URL: &'static str = "https://dumps.wikimedia.org/backup-index.html";
static DUMP_BASE_URL: &'static str = "https://ftp.acc.umu.se/mirror/wikimedia.org/dumps";

pub fn dump_status_file(base_url: &DumpBaseUrl, language_code: &LanguageCode, date: &str) -> Result<Url> {
    let base_url = base_url.as_str();
    let language_abbreviation = language_code.to_wiktionary_abbreviation();
    Ok(Url::parse(&format!("{base_url}/{language_abbreviation}wiktionary/{date}/dumpstatus.json"))?)
}

pub fn available_dates(base_url: &DumpBaseUrl, language_code: &LanguageCode) -> Result<Url> {
    let base_url = base_url.as_str();
    let language_abbreviation = language_code.to_wiktionary_abbreviation();
    Ok(Url::parse(&format!("{base_url}/{language_abbreviation}wiktionary/"))?)
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
pub enum DumpBaseUrl {
    Default,
    Custom(String),
}

impl DumpBaseUrl {
    pub fn as_str(&self) -> &str {
        match self {
            DumpBaseUrl::Default => DUMP_BASE_URL,
            DumpBaseUrl::Custom(custom) => custom,
        }
    }
}