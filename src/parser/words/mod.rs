use lazy_static::lazy_static;
use regex::Regex;
use wikitext_parser::Section;

use crate::error::Error;
use crate::parser::Wikitext;

lazy_static! {
    static ref IGNORED_PATTERN: Regex =
        Regex::new("(Wiktionary:|Appendix:|Help:|Rhymes:|Template:|MediaWiki:|Citations:|Module:|Reconstruction:|Thesaurus:|Concordance:).*|.*(/derived terms)").unwrap();
    static ref WORD_TYPE_PATTERN: Regex =
        Regex::new("Word|Noun|Proper noun|Dependent noun|Prenoun|Participle|Gerund(ive)?|Verb|Preverb|Predicative|Conjugation|Adjective|Comparative-only adjectives|Determinative|Adverb|Adnominal|Inflection|Pronoun|Preposition|Postposition|Ambiposition|Circumposition|Conjunction|Initial|Prefix|Suffix|Final|Affix|Infix|Interfix|Circumfix|Clitic|Article|Particle|Locative|Determiner|Classifier|Subordinate modifier|Contraction|Combining form|Compound part|Enclitic|Relative|Phrase|Propositional phrase|Proverb|Idiom|Honorific title|Ideophone|Phonogram|Onomatopoeia|Phoneme|Ligature|Syllable|Letter|Symbol|Counter|Number|Numeral|Multiple parts of speech|Punctuation mark|Diacritical mark|Root")
            .unwrap();
    static ref IGNORED_LANGUAGE_PATTERN: Regex = Regex::new("Translingual").unwrap();
    static ref IGNORED_SUBSECTION_PATTERN: Regex = Regex::new("Variant spellings|Relational forms|Spelling variants|Other usage|Other versions|Possessed forms|Graphical notes|Design|Echo word|From|Description|Derived characters|Derived|Derivatives|Alternate spelling|Accentuation notes|Accentological notes|Usage|Citations?|Examples?|Sources|User notes?|Work to be done|Stem|Sign values|Reconstruction|Production|Logogram|Holonyms?|Meronyms|Forms?|Dialectal synonyms?|Decadents?|Abbreviations?|Borrowed terms?|External (L|l)inks?|Related words?|Standard form|Nom glyph origin|Readings?|Synonyms?|Antonyms?|Hyponyms?|Hypernyms?|Paronyms?|Translations?|Coordinate terms?|Dialectal variants?|Romanization|Statistics?|Declension|Alternative scripts?|Phrasal verbs?|Trivia|Han character|Hanzi|Glyph origin|Definitions?|Compounds?|Descendants?|Kanji|Hanja|Notes?|Derived (t|T)erms?|Usage notes|Alternative forms|Alternative|Etymology|Pronunciation( [1-9][0-9]*)?|Further reading|Anagrams|References?|Refs|Further references?|See ?(a|A)lso|Mutation|Interjection|Quotations|Gallery|Related (t|T)erms?").unwrap();
}

pub struct Word {
    /// The word itself.
    /// Multiple `Word`s may have the same `word` if they are of a different language or type.
    pub word: String,

    /// The english name of the language this word is from.
    /// While different languages may contain the same words, there will be a separate word instance for each.
    pub language_english_name: String,

    /// The word type, as declared by wiktionary.
    /// While a word may have multiple types, there will be a separate word instance for each.
    pub word_type: String,
}

pub fn wikitext_to_words(
    title: &str,
    wikitext: &Wikitext,
    mut result_consumer: impl FnMut(Word),
    mut error_consumer: impl FnMut(Error),
) {
    if IGNORED_PATTERN.is_match(title) {
        // silently ignore non-words
        return;
    }

    let root_section = &wikitext.root_section;

    if root_section.headline.level == 1 {
        let word = &root_section.headline.label;

        for subsection in &root_section.subsections {
            parse_language_subsection(word, subsection, &mut result_consumer, &mut error_consumer);
        }
    } else {
        error_consumer(Error::Other(
            "Root section is not at headline level 1".to_string(),
        ));
    }
}

fn parse_language_subsection(
    word: &str,
    language_subsection: &Section,
    result_consumer: &mut impl FnMut(Word),
    error_consumer: &mut impl FnMut(Error),
) {
    let language_english_name = language_subsection.headline.label.as_str();
    if IGNORED_LANGUAGE_PATTERN.is_match(language_english_name) {
        // silently ignore high-level metalanguages
        return;
    }

    if language_subsection.subsections.is_empty() {
        result_consumer(Word {
            word: word.to_string(),
            language_english_name: language_english_name.to_string(),
            word_type: "Unknown".to_string(),
        });
    } else {
        let mut toplevel_details = false;
        let mut bottomlevel_details = false;
        let mut bottomlevel_errors = Vec::new();

        for unknown_subsection in &language_subsection.subsections {
            if unknown_subsection.headline.label == "Etymology"
                || WORD_TYPE_PATTERN.is_match(&unknown_subsection.headline.label)
            {
                toplevel_details = true;
            } else if unknown_subsection.headline.label != "Etymology"
                && unknown_subsection.headline.label.starts_with("Etymology")
            {
                bottomlevel_details = true;
                parse_details_subsection(
                    word,
                    language_english_name,
                    unknown_subsection,
                    result_consumer,
                    error_consumer,
                );
            } else if IGNORED_SUBSECTION_PATTERN.is_match(&unknown_subsection.headline.label) {
                // ignore
            } else {
                bottomlevel_errors.push(Error::Other(format!(
                    "Unknown subsection of language: {}",
                    unknown_subsection.headline.label
                )));
            }
        }

        if toplevel_details {
            parse_details_subsection(
                word,
                language_english_name,
                language_subsection,
                result_consumer,
                error_consumer,
            );
        }

        if toplevel_details && bottomlevel_details {
            error_consumer(Error::Other(format!(
                "Found both toplevel and bottomlevel details for language {language_english_name}"
            )));
        }

        if bottomlevel_details {
            for error in bottomlevel_errors {
                error_consumer(error);
            }
        }
    }
}

fn parse_details_subsection(
    word: &str,
    language_english_name: &str,
    details_subsection: &Section,
    result_consumer: &mut impl FnMut(Word),
    error_consumer: &mut impl FnMut(Error),
) {
    for details_section in &details_subsection.subsections {
        let word_type = &details_section.headline.label;
        if WORD_TYPE_PATTERN.is_match(word_type) {
            result_consumer(Word {
                word: word.to_string(),
                language_english_name: language_english_name.to_string(),
                word_type: word_type.clone(),
            });
        } else if IGNORED_SUBSECTION_PATTERN.is_match(word_type) {
            // ignore
        } else {
            error_consumer(Error::Other(format!(
                "Unknown details subsection: {word_type}"
            )));
        }
    }
}
