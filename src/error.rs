use std::{fmt::Display, string::FromUtf8Error};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    ReqwestError(reqwest::Error),
    RegexError(regex::Error),
    UrlParseError(url::ParseError),
    SerdeJsonError(serde_json::Error),
    IoError(std::io::Error),
    FromUtf8Error(std::string::FromUtf8Error),
    QuickXmlError(quick_xml::Error),
    QuickXmlAttributeError(quick_xml::events::attributes::AttrError),
    WikitextParserError {
        /// The error returned by the parser.
        error: Box<wikitext_parser::ParserError>,
        /// The name of the page.
        page_name: String,
        /// The page content causing the error.
        page_content: String,
    },

    /// The given english language name is unknown.
    UnknownEnglishLanguageName(String),

    /// The given wiktionary language abbreviation is unknown.
    UnknownWiktionaryLanguageAbbreviation(String),

    /// An error described by a string instead of a variant.
    Other(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ReqwestError(error) => write!(f, "error sending http request: {error}"),
            Error::RegexError(error) => write!(f, "regex error: {error}"),
            Error::UrlParseError(error) => write!(f, "error parsing url: {error}"),
            Error::SerdeJsonError(error) => write!(f, "json error: {error}"),
            Error::IoError(error) => write!(f, "io error: {error}"),
            Error::FromUtf8Error(error) => write!(f, "error parsing utf-8: {error}"),
            Error::QuickXmlError(error) => write!(f, "error parsing xml: {error}"),
            Error::QuickXmlAttributeError(error) => {
                write!(f, "error parsing xml attribute: {error}")
            }
            Error::WikitextParserError {
                error, page_name, ..
            } => write!(f, "error parsing page {page_name:?}: {error}"),
            Error::UnknownEnglishLanguageName(name) => {
                write!(f, "unknown English language name: {name:?}")
            }
            Error::UnknownWiktionaryLanguageAbbreviation(abbreviation) => write!(
                f,
                "unknown wiktionary language abbreviation: {abbreviation}"
            ),
            Error::Other(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

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

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::IoError(error)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(error: FromUtf8Error) -> Self {
        Self::FromUtf8Error(error)
    }
}

impl From<quick_xml::Error> for Error {
    fn from(error: quick_xml::Error) -> Self {
        Self::QuickXmlError(error)
    }
}

impl From<quick_xml::events::attributes::AttrError> for Error {
    fn from(error: quick_xml::events::attributes::AttrError) -> Self {
        Self::QuickXmlAttributeError(error)
    }
}
