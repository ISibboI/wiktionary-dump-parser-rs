use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("error sending http request: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("regex error: {0}")]
    RegexError(#[from] regex::Error),
    #[error("error parsing url: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("json error: {0}")]
    SerdeJsonError(#[from] serde_json::Error),
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("error parsing utf-8: {0}")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
    #[error("error parsing xml: {0}")]
    QuickXmlError(#[from] quick_xml::Error),
    #[error("error parsing xml attribute: {0}")]
    QuickXmlAttributeError(#[from] quick_xml::events::attributes::AttrError),
    #[error("error parsing page {page_name:?}: {error}")]
    WikitextParserError {
        /// The error returned by the parser.
        error: Box<wikitext_parser::ParserError>,
        /// The name of the page.
        page_name: String,
        /// The page content causing the error.
        page_content: String,
    },

    /// The given english language name is unknown.
    #[error("unknown English language name: {0:?}")]
    UnknownEnglishLanguageName(String),

    /// The given wiktionary language abbreviation is unknown.
    #[error("unknown wiktionary language abbreviation: {0}")]
    UnknownWiktionaryLanguageAbbreviation(String),

    /// An error described by a string instead of a variant.
    #[error("{0}")]
    Other(String),

    #[error("error consuming parsed word: {source}")]
    WordConsumer {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
