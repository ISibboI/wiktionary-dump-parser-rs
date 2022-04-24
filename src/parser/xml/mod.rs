use crate::error::Result;
use log::{debug, trace};
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Reader;
use std::io::BufRead;

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

pub fn read_relevant_event<'reader, 'buffer, 'result>(
    reader: &'reader mut Reader<impl BufRead>,
    buffer: &'buffer mut Vec<u8>,
) -> Result<RelevantEvent<'result>> {
    let relevant_event;

    loop {
        match reader.read_event(buffer)? {
            Event::Start(tag) => {
                relevant_event =
                    RelevantEvent::Start(BytesStart::owned(tag.to_vec(), tag.name().len()));
                break;
            }
            Event::End(tag) => {
                relevant_event = RelevantEvent::End(BytesEnd::owned(tag.to_vec()));
                break;
            }
            Event::Empty(tag) => {
                relevant_event =
                    RelevantEvent::Empty(BytesStart::owned(tag.to_vec(), tag.name().len()));
                break;
            }
            Event::Text(text) => {
                if text.iter().any(|byte| !byte.is_ascii_whitespace()) {
                    relevant_event = RelevantEvent::Text(BytesText::from_escaped(text.to_vec()));
                    break;
                }
            }
            Event::Comment(comment) => {
                debug!("Found comment {comment:?}");
            }
            Event::CData(cdata) => {
                debug!("Found CDATA {cdata:?}");
            }
            Event::Decl(decl) => {
                debug!("Found XML declaration {decl:?}");
            }
            Event::PI(pi) => {
                debug!("Found processing instruction {pi:?}");
            }
            Event::DocType(doc_type) => {
                debug!("Found DOCTYPE {doc_type:?}");
            }
            Event::Eof => {
                relevant_event = RelevantEvent::Eof;
                break;
            }
        }
    }

    trace!("Read relevant event {relevant_event:?}");
    Ok(relevant_event)
}
