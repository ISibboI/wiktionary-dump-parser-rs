use std::string::FromUtf8Error;

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
    WikitextParserError(wikitext_parser::ParserError),

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

impl From<wikitext_parser::ParserError> for Error {
    fn from(error: wikitext_parser::ParserError) -> Self {
        Self::WikitextParserError(error)
    }
}
