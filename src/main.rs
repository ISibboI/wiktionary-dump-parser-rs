use clap::Parser;
use log::{info, LevelFilter};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};
use wiktionary_dump_parser::error::Result;
use wiktionary_dump_parser::{list_wiktionary_dump_languages, DumpIndexUrl};

#[derive(Parser)]
struct Configuration {
    #[clap(subcommand)]
    command: CliCommand,
}

#[derive(clap::Subcommand)]
enum CliCommand {
    /// Lists the languages that wiktionary is available in.
    ListAvailableLanguages,
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
