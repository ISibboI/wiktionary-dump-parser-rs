pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    ReqwestError(reqwest::Error),
    RegexError(regex::Error),
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
