use std::io::BufRead;
use log::debug;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Reader;
use crate::error::Result;

#[derive(Clone, Debug)]
pub enum RelevantEvent<'a> {
    /// Start tag (with attributes) `<tag attr="value">`.
    Start(BytesStart<'a>),
    /// End tag `</tag>`.
    End(BytesEnd<'a>),
    /// Empty element tag (with attributes) `<tag attr="value" />`.
    Empty(BytesStart<'a>),
    /// Character data between `Start` and `End` element.
    Text(BytesText<'a>),
    /// End of XML document.
    Eof,
}

pub fn read_relevant_event<'reader, 'buffer, 'result>(reader: &'reader mut Reader<impl BufRead>, buffer: &'buffer mut Vec<u8>) -> Result<RelevantEvent<'result>> {
    loop {
        match reader.read_event(buffer)? {
            Event::Start(tag) => return Ok(RelevantEvent::Start(BytesStart::owned(tag.to_vec(), tag.name().len()))),
            Event::End(tag) => return Ok(RelevantEvent::End(BytesEnd::owned(tag.to_vec()))),
            Event::Empty(tag) => return Ok(RelevantEvent::Empty(BytesStart::owned(tag.to_vec(), tag.name().len()))),
            Event::Text(text) => return Ok(RelevantEvent::Text(BytesText::from_escaped(text.to_vec()))),
            Event::Comment(comment) => { debug!("Found comment {comment:?}"); }
            Event::CData(cdata) => { debug!("Found CDATA {cdata:?}"); }
            Event::Decl(decl) => { debug!("Found XML declaration {decl:?}"); }
            Event::PI(pi) => { debug!("Found processing instruction {pi:?}"); }
            Event::DocType(doc_type) => { debug!("Found DOCTYPE {doc_type:?}"); }
            Event::Eof => return Ok(RelevantEvent::Eof),
        }
    }
}
