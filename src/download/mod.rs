use crate::error::{Error, Result};
use digest::Digest;
use futures_util::stream::StreamExt;
use lexiclean::Lexiclean;
use log::{debug, info, warn};
use md5::Md5;
use num_integer::Integer;
use sha1::Sha1;
use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::time::Duration;
use tokio::time::Instant;
use url::Url;

pub async fn download_file_with_progress_log(
    from_url: &Url,
    to_path: impl Into<PathBuf>,
    expected_size: usize,
    progress_delay: u64,
    md5: Option<&str>,
    sha1: Option<&str>,
) -> Result<PathBuf> {
    let mut to_path = to_path.into();
    if to_path.is_relative() {
        let mut current_dir = env::current_dir()?;
        current_dir.push(to_path);
        to_path = current_dir;
    }

    let to_path = to_path.lexiclean();
    let to_path_string = to_path.to_string_lossy();
    info!("Downloading file from '{from_url}' to '{to_path_string}'");

    debug!("Requesting file from server");
    let url_connection = reqwest::get(from_url.clone()).await?;
    let expected_content_length: u64 = expected_size.try_into().map_err(|_| {
        Error::Other(format!(
            "File size {} is larger than u64::MAX {}",
            expected_size,
            u64::MAX
        ))
    })?;
    let expected_content_length_mib = expected_content_length / (1024 * 1024);
    if let Some(content_length) = url_connection.content_length() {
        if content_length != expected_content_length {
            return Err(Error::Other(format!("Content length mismatch, status file declares {expected_content_length}, but server declares {content_length}")));
        }
    } else {
        warn!("Missing content length header for '{from_url}'");
    }

    if let Some(parent_dirs) = to_path.parent() {
        let parent_dirs_string = parent_dirs.to_string_lossy();
        debug!("Creating parent dirs '{parent_dirs_string}'");
        tokio::fs::create_dir_all(parent_dirs).await?;
    } else {
        debug!("Skipping creating parent dirs, because the target path does not have any '{to_path_string}'");
    }

    debug!("Creating local file");
    let mut output_file = File::create(&to_path).await?;

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

    let mut md5_digest = Md5::default();
    let mut sha1_digest = Sha1::default();

    while let Some(chunk) = input_stream.next().await {
        let chunk = chunk?;

        if md5.is_some() {
            md5_digest.update(&chunk);
        }
        if sha1.is_some() {
            sha1_digest.update(&chunk);
        }

        output_file.write_all(&chunk).await?;

        let now = Instant::now();
        if last_progress_output + Duration::from_secs(progress_delay) < now {
            let current_content_length = output_file.metadata().await?.len();
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

    if let Some(md5) = md5 {
        debug!("Verifying md5 checksum");
        let md5_digest = format!("{:x}", md5_digest.finalize());
        if md5_digest == md5 {
            debug!("Md5 checksum matches");
        } else {
            return Err(Error::Other(format!(
                "Md5 checksum is '{md5_digest}', but should be '{md5}'"
            )));
        }
    } else {
        debug!("No md5 checksum given, skipping verification");
    }

    if let Some(sha1) = sha1 {
        debug!("Verifying sha1 checksum");
        let sha1_digest = format!("{:x}", sha1_digest.finalize());
        if sha1_digest == sha1 {
            debug!("Sha1 checksum matches");
        } else {
            return Err(Error::Other(format!(
                "Sha1 checksum is '{sha1_digest}', but should be '{sha1}'"
            )));
        }
    } else {
        debug!("No sha1 checksum given, skipping verification");
    }

    let output_file_length = output_file.metadata().await?.len();
    if output_file_length != expected_content_length {
        return Err(Error::Other(format!("Content length mismatch, status file declares {expected_content_length}, but we received {output_file_length}")));
    }

    drop(output_file);

    info!("Finished downloading file from '{from_url}' to '{to_path_string}'");
    Ok(to_path)
}
