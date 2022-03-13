use crate::error::{Error, Result};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LanguageCode {
    English,
    French,
    Russian,
    German,
    Finnish,
}

impl LanguageCode {
    pub fn from_wiktionary_abbreviation(string: &str) -> Result<Self> {
        Ok(match string {
            "en" => Self::English,
            "fr" => Self::French,
            "ru" => Self::Russian,
            "de" => Self::German,
            "fi" => Self::Finnish,
            unknown => {
                return Err(Error::UnknownWiktionaryLanguageAbbreviation(
                    unknown.to_string(),
                ))
            }
        })
    }

    pub fn to_wiktionary_abbreviation(&self) -> &'static str {
        match self {
            LanguageCode::English => "en",
            LanguageCode::French => "fr",
            LanguageCode::Russian => "ru",
            LanguageCode::German => "de",
            LanguageCode::Finnish => "fi",
        }
    }

    pub fn from_english_name(string: &str) -> Result<Self> {
        Ok(match string {
            "English" => Self::English,
            "French" => Self::French,
            "Russian" => Self::Russian,
            "German" => Self::German,
            "Finnish" => Self::Finnish,
            unknown => return Err(Error::UnknownEnglishLanguageName(unknown.to_string())),
        })
    }
}
