use wikitext_parser::Section;

use crate::error::Error;
use crate::parser::Wikitext;

pub struct Word {
    pub word: String,
}

pub fn wikitext_to_words(
    wikitext: &Wikitext,
    mut result_consumer: impl FnMut(Word),
    mut error_consumer: impl FnMut(Error),
) {
    let root_section = &wikitext.root_section;

    if root_section.headline.level == 1 {
        let word = &root_section.headline.label;

        for subsection in &root_section.subsections {
            parse_language_subsection(
                word.clone(),
                subsection,
                &mut result_consumer,
                &mut error_consumer,
            );
        }
    } else {
        error_consumer(Error::Other(
            "Root section is not at headline level 1".to_string(),
        ));
    }
}

fn parse_language_subsection(
    word: String,
    subsection: &Section,
    result_consumer: &mut impl FnMut(Word),
    error_consumer: &mut impl FnMut(Error),
) {
    todo!()
}
