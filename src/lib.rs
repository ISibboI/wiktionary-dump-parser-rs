use std::collections::BTreeMap;
use itertools::Itertools;
use crate::language_code::LanguageCode;
use error::Result;
use lazy_static::lazy_static;
use log::{debug, trace, warn};
use regex::Regex;
use crate::urls::{available_dates, dump_status_file, DumpBaseUrl, DumpIndexUrl};
use serde::{Serialize, Deserialize};
use crate::error::Error;

pub mod urls;
pub mod error;
pub mod language_code;


lazy_static! {
    static ref LIST_WIKTIONARY_DUMP_LANGUAGES_REGEX: Regex =
        Regex::new(r#"<a href="([a-z\-]{2,20})wiktionary/[0-9]{8}">"#).unwrap();
    static ref LIST_AVAILABLE_DATES_REGEX: Regex =
        Regex::new(r#"<a href=".*([0-9]{8})/?">"#).unwrap();
}

pub async fn list_wiktionary_dump_languages(url: &DumpIndexUrl) -> Result<Vec<LanguageCode>> {
    let body = reqwest::get(url.as_str()).await?.text().await?;
    trace!("{body}");
    debug!(
        "language_regex: {:?}",
        *LIST_WIKTIONARY_DUMP_LANGUAGES_REGEX
    );
    Ok(LIST_WIKTIONARY_DUMP_LANGUAGES_REGEX
        .captures_iter(&body)
        .filter_map(|captures| {
            let abbreviation = &captures[1];
            if let Ok(language_code) = LanguageCode::from_wiktionary_abbreviation(abbreviation) {
                Some(language_code)
            } else {
                warn!("Unknown language abbreviation '{abbreviation}'");
                None
            }
        })
        .collect())
}

pub async fn list_available_dates(base_url: &DumpBaseUrl, language_code: &LanguageCode) -> Result<Vec<String>> {
    let url = available_dates(base_url, language_code)?;
    let body = reqwest::get(url).await?.text().await?;
    trace!("{body}");
    debug!("available_dates_regex: {:?}", *LIST_AVAILABLE_DATES_REGEX);
    Ok(LIST_AVAILABLE_DATES_REGEX.captures_iter(&body).map(|captures| {
        captures[1].to_string()
    }).sorted().unique().collect())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpStatusFile {
    version: String,
    jobs: BTreeMap<String, DumpStatusFileEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpStatusFileEntry {
    status: String,
    updated: String,
    #[serde(default)]
    files: BTreeMap<String, DumpStatusFileEntryFile>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DumpStatusFileEntryFile {
    #[serde(default)]
    size: usize,
    #[serde(default)]
    url: String,
    #[serde(default)]
    md5: String,
    #[serde(default)]
    sha1: String,
}

pub async fn download_language(base_url: &DumpBaseUrl, language_code: &LanguageCode) -> Result<()> {
    let available_dates = list_available_dates(base_url, language_code).await?;
    debug!("Available dates: {available_dates:?}");

    if available_dates.len() < 2 {
        return Err(Error::Other(format!("Less than two available dates: {available_dates:?}")));
    }
    let date = &available_dates[available_dates.len() - 2];
    debug!("Selected second to last date '{date}'");

    let url = dump_status_file(base_url, language_code, date)?;
    let body = reqwest::get(url).await?.text().await?;
    trace!("{body}");
    let dump_status_file: DumpStatusFile = serde_json::from_str(&body)?;
    Ok(())
}