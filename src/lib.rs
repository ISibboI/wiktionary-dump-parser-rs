use crate::error::Error;
use crate::language_code::LanguageCode;
use crate::urls::{available_dates, dump_status_file, dump_url, DumpBaseUrl, DumpIndexUrl};
use error::Result;
use futures_util::stream::StreamExt;
use itertools::Itertools;
use lazy_static::lazy_static;
use lexiclean::Lexiclean;
use log::{debug, info, trace, warn};
use num_integer::Integer;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs};
use tokio::time::Instant;

pub mod error;
pub mod language_code;
pub mod urls;

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

pub async fn download_language(
    base_url: &DumpBaseUrl,
    language_code: &LanguageCode,
    target_directory: impl Into<PathBuf>,
    progress_delay: u64,
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
    if target_file.is_relative() {
        let mut current_dir = env::current_dir()?;
        current_dir.push(target_file);
        target_file = current_dir;
    }
    target_file.push(language_abbreviation);
    target_file.push(date);
    target_file.push(file_name);
    let target_file = target_file.lexiclean();
    let target_file_string = target_file.to_string_lossy();
    info!("Downloading file from '{url}' to '{target_file_string}'");

    debug!("Requesting file from server");
    let url_connection = reqwest::get(url.clone()).await?;
    let expected_content_length: u64 = properties.size.try_into().map_err(|_| {
        Error::Other(format!(
            "File size {} is larger than u64::MAX {}",
            properties.size,
            u64::MAX
        ))
    })?;
    let expected_content_length_mib = expected_content_length / (1024 * 1024);
    if let Some(content_length) = url_connection.content_length() {
        if content_length != expected_content_length {
            return Err(Error::Other(format!("Content length mismatch, status file declares {expected_content_length}, but server declares {content_length}")));
        }
    } else {
        warn!("Missing content length header for '{url}'");
    }

    if let Some(parent_dirs) = target_file.parent() {
        let parent_dirs_string = parent_dirs.to_string_lossy();
        debug!("Creating parent dirs '{parent_dirs_string}'");
        fs::create_dir_all(parent_dirs)?;
    } else {
        debug!("Skipping creating parent dirs, because the target path does not have any '{target_file_string}'");
    }

    debug!("Creating local file");
    let mut output_file = File::create(&target_file)?;

    debug!("Starting download");
    let mut input_stream = url_connection.bytes_stream();
    let mut last_progress_output = Instant::now();
    let mut last_content_lengths: VecDeque<(u64, Instant)> = VecDeque::new();
    let progress_delay = if progress_delay == 0 {
        warn!("Progress delay was set to zero, but needs to be at least one. Changing to one.");
        1
    } else {
        progress_delay
    };
    // Cannot fail as maximum value is 60.
    let retained_content_length_amount: usize = (60 / progress_delay).max(1).try_into().unwrap();
    last_content_lengths.push_back((0, Instant::now()));
    while let Some(chunk) = input_stream.next().await {
        let chunk = chunk?;
        output_file.write_all(&chunk)?;

        let now = Instant::now();
        if last_progress_output + Duration::from_secs(progress_delay) < now {
            let current_content_length = output_file.metadata()?.len();
            let current_content_length_mib = current_content_length / (1024 * 1024);
            let fraction = current_content_length as f64 / expected_content_length as f64;
            let percent = fraction * 100.0;

            let eta = if let Some((eta_content_length, eta_instant)) = last_content_lengths.front()
            {
                let eta_content_length_fraction = (current_content_length - eta_content_length)
                    as f64
                    / expected_content_length as f64;
                let eta_multiplier = (1.0 - fraction) / eta_content_length_fraction;
                let eta_duration_seconds = (now - *eta_instant).as_secs_f64() * eta_multiplier;
                while last_content_lengths.len() >= retained_content_length_amount {
                    last_content_lengths.pop_front();
                }

                if eta_duration_seconds < 1.0 {
                    "<1s".to_string()
                } else {
                    let eta_duration_seconds = eta_duration_seconds.round() as u64;
                    let (eta_duration_minutes, seconds) = eta_duration_seconds.div_rem(&60);
                    let (eta_duration_hours, minutes) = eta_duration_minutes.div_rem(&60);
                    let (days, hours) = eta_duration_hours.div_rem(&24);

                    if days > 9999 {
                        ">9999d".to_string()
                    } else if days > 0 {
                        format!("{days}d {hours}h")
                    } else if hours > 0 {
                        format!("{hours}h {minutes}m")
                    } else if minutes > 0 {
                        format!("{minutes}m {seconds}s")
                    } else {
                        format!("{seconds}s")
                    }
                }
            } else {
                "-".to_string()
            };

            info!("{percent:.1}% {current_content_length_mib}MiB/{expected_content_length_mib}MiB ETA {eta}");
            last_progress_output = now;
            last_content_lengths.push_back((current_content_length, now));
        }
    }
    debug!("Download finished");
    drop(input_stream);

    let output_file_length = output_file.metadata()?.len();
    if output_file_length != expected_content_length {
        return Err(Error::Other(format!("Content length mismatch, status file declares {expected_content_length}, but we received {output_file_length}")));
    }

    drop(output_file);

    info!("Finished downloading file from '{url}' to '{target_file_string}'");
    Ok(())
}
