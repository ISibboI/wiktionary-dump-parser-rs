#![allow(clippy::useless_format)]

use clap::Parser;
use log::{info, LevelFilter};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};
use std::path::PathBuf;
use wiktionary_dump_parser::error::{Error, Result};
use wiktionary_dump_parser::language_code::LanguageCode;
use wiktionary_dump_parser::urls::{DumpBaseUrl, DumpIndexUrl};
use wiktionary_dump_parser::{
    download_language, list_wiktionary_dump_languages, parser::parse_dump_file,
};

#[derive(Parser)]
struct Configuration {
    #[clap(long, default_value = "Info")]
    log_level: LevelFilter,

    #[clap(subcommand)]
    command: CliCommand,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    /// Lists the languages that wiktionary is available in.
    ListAvailableLanguages,

    /// Completely downloads a single language.
    DownloadLanguage {
        #[clap(long)]
        english_name: Option<String>,
        #[clap(long)]
        wiktionary_abbreviation: Option<String>,
        #[clap(long, default_value = ".")]
        target_directory: PathBuf,
        #[clap(long, default_value = "10")]
        progress_delay: u64,
    },

    ParseDumpFile {
        #[clap(long)]
        input_file: PathBuf,
        #[clap(long)]
        output_file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let configuration = Configuration::parse();
    initialise_logging(configuration.log_level);

    match configuration.command {
        CliCommand::ListAvailableLanguages => {
            for language_code in list_wiktionary_dump_languages(&DumpIndexUrl::Default).await? {
                println!("{language_code:?}")
            }
        }

        CliCommand::DownloadLanguage {
            english_name,
            wiktionary_abbreviation,
            target_directory,
            progress_delay,
        } => {
            let language_code = match (english_name, wiktionary_abbreviation) {
                (Some(english_name), None) => LanguageCode::from_english_name(&english_name)?,
                (None, Some(wiktionary_abbreviation)) => LanguageCode::from_wiktionary_abbreviation(&wiktionary_abbreviation)?,
                (None, None) => return Err(Error::Other(format!("No language to download specified."))),
                (Some(english_name), Some(wiktionary_abbreviation)) => return Err(Error::Other(format!("Specified both the english name '{english_name}' and the wiktionary abbreviation '{wiktionary_abbreviation}' of the language to download."))),
            };

            info!("Downloading language {language_code:?}");
            download_language(
                &DumpBaseUrl::Default,
                &language_code,
                &target_directory,
                progress_delay,
            )
            .await?;
        }

        CliCommand::ParseDumpFile {
            input_file,
            output_file,
        } => {
            info!("Parsing dump file {input_file:?} into {output_file:?}");
            parse_dump_file(&input_file, &output_file).await?;
        }
    }

    info!("Terminating");
    Ok(())
}

fn initialise_logging(log_level: LevelFilter) {
    CombinedLogger::init(vec![TermLogger::new(
        log_level,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    info!("Logging initialised successfully");
}
