pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    ReqwestError(reqwest::Error),
    RegexError(regex::Error),
    UrlParseError(url::ParseError),
    SerdeJsonError(serde_json::Error),

    /// The given english language name is unknown.
    UnknownEnglishLanguageName(String),

    /// The given wiktionary language abbreviation is unknown.
    UnknownWiktionaryLanguageAbbreviation(String),

    /// An error described by a string instead of a variant.
    Other(String),
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::ReqwestError(error)
    }
}

impl From<regex::Error> for Error {
    fn from(error: regex::Error) -> Self {
        Self::RegexError(error)
    }
}

impl From<url::ParseError> for Error {
    fn from(error: url::ParseError) -> Self {
        Self::UrlParseError(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::SerdeJsonError(error)
    }
}