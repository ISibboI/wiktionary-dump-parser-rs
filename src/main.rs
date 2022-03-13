use clap::Parser;
use log::{info, LevelFilter};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};
use wiktionary_dump_parser::error::{Error, Result};
use wiktionary_dump_parser::{list_wiktionary_dump_languages, download_language};
use wiktionary_dump_parser::language_code::LanguageCode;
use wiktionary_dump_parser::urls::{DumpBaseUrl, DumpIndexUrl};

#[derive(Parser)]
struct Configuration {
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
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    initialise_logging();

    let configuration = Configuration::parse();

    match configuration.command {
        CliCommand::ListAvailableLanguages => {
            for language_code in list_wiktionary_dump_languages(&DumpIndexUrl::Default).await? {
                println!("{language_code:?}")
            }
        }

        CliCommand::DownloadLanguage {english_name, wiktionary_abbreviation} => {
            let language_code = match (english_name, wiktionary_abbreviation) {
                (Some(english_name), None) => LanguageCode::from_english_name(&english_name)?,
                (None, Some(wiktionary_abbreviation)) => LanguageCode::from_wiktionary_abbreviation(&wiktionary_abbreviation)?,
                (None, None) => return Err(Error::Other(format!("No language to download specified."))),
                (Some(english_name), Some(wiktionary_abbreviation)) => return Err(Error::Other(format!("Specified both the english name '{english_name}' and the wiktionary abbreviation '{wiktionary_abbreviation}' of the language to download."))),
            };

            info!("Downloading language '{language_code:?}'");
            download_language(&DumpBaseUrl::Default, &language_code).await?;
        }
    }

    info!("Terminating");
    Ok(())
}

fn initialise_logging() {
    CombinedLogger::init(vec![TermLogger::new(
        if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        },
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    info!("Logging initialised successfully");
}
