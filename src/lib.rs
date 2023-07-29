#![allow(clippy::useless_format)]

use crate::download::download_file_with_progress_log;
use crate::error::Error;
use crate::language_code::LanguageCode;
use crate::urls::{available_dates, dump_status_file, dump_url, DumpBaseUrl, DumpIndexUrl};
use error::Result;
use itertools::Itertools;
use lazy_static::lazy_static;
use log::{debug, info, trace, warn};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod download;
pub mod error;
pub mod language_code;
pub mod parser;
pub mod urls;

lazy_static! {
    static ref LIST_WIKTIONARY_DUMP_LANGUAGES_REGEX: Regex =
        Regex::new(r#"<a href="([a-z\-]{2,20})wiktionary/[0-9]{8}">"#).unwrap();
    static ref LIST_AVAILABLE_DATES_REGEX: Regex =
        Regex::new(r#"<a href=".*([0-9]{8})/?">"#).unwrap();
}

/// Query wiktionary to get a list of languages that wiktionary dumps are available in.
/// These are the languages wiktionary itself exists in, not the languages it has data about.
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

/// Given a language code, list the available dates for which dumps exist.
pub async fn list_available_dates(
    base_url: &DumpBaseUrl,
    language_code: &LanguageCode,
) -> Result<Vec<String>> {
    let url = available_dates(base_url, language_code)?;
    let body = reqwest::get(url).await?.text().await?;
    trace!("{body}");
    debug!("available_dates_regex: {:?}", *LIST_AVAILABLE_DATES_REGEX);
    Ok(LIST_AVAILABLE_DATES_REGEX
        .captures_iter(&body)
        .map(|captures| captures[1].to_string())
        .sorted()
        .unique()
        .collect())
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
    files: BTreeMap<String, DumpStatusFileEntryFile>,
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

/// Download the latest dump of wiktionary in the given language.
pub async fn download_language(
    base_url: &DumpBaseUrl,
    language_code: &LanguageCode,
    target_directory: impl Into<PathBuf>,
    progress_delay_seconds: u64,
) -> Result<()> {
    let available_dates = list_available_dates(base_url, language_code).await?;
    debug!("Available dates: {available_dates:?}");

    if available_dates.len() < 2 {
        return Err(Error::Other(format!(
            "Less than two available dates: {available_dates:?}"
        )));
    }
    let date = &available_dates[available_dates.len() - 2];
    debug!("Selected second to last date '{date}'");

    let url = dump_status_file(base_url, language_code, date)?;
    let body = reqwest::get(url).await?.text().await?;
    trace!("{body}");
    let dump_status_file: DumpStatusFile = serde_json::from_str(&body)?;
    trace!("{dump_status_file:#?}");

    let dump_status_file_version = &dump_status_file.version;
    if dump_status_file_version != "0.8" {
        return Err(Error::Other(format!("Wrong dump status file version '{dump_status_file_version}', currently only 0.8 is supported.")));
    }

    let articles_dump = dump_status_file.jobs.get("articlesdump").ok_or_else(|| {
        Error::Other(format!(
            "Dump status file misses job entry for 'articlesdump'"
        ))
    })?;
    trace!("{articles_dump:#?}");

    let articles_dump_status = &articles_dump.status;
    if articles_dump_status != "done" {
        return Err(Error::Other(format!(
            "Wrong articlesdump status '{articles_dump_status}', expected 'done'."
        )));
    }
    let articles_dump_file_amount = articles_dump.files.len();
    if articles_dump_file_amount != 1 {
        return Err(Error::Other(format!(
            "Wrong articlesdump file amount {articles_dump_file_amount}, expected 1."
        )));
    }

    // Unwrap cannot panic because we abort if there is not exactly one entry.
    let (file_name, properties) = articles_dump.files.iter().next().unwrap();
    let url = dump_url(base_url, &properties.url)?;
    let language_abbreviation = language_code.to_wiktionary_abbreviation();
    let mut target_file = target_directory.into();
    target_file.push(language_abbreviation);
    target_file.push(date);
    target_file.push(file_name);

    if target_file.exists() {
        info!("Skipping download, because file exists already.");
    } else {
        download_file_with_progress_log(
            &url,
            target_file,
            properties.size,
            progress_delay_seconds,
            Some(&properties.md5),
            Some(&properties.sha1),
        )
        .await?;
    }

    Ok(())
}
