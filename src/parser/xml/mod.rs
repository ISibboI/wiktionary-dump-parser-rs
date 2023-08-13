use crate::error::Result;
use log::{debug, trace};
use quick_xml::{
    events::{BytesEnd, BytesStart, Event},
    Reader,
};
use tokio::io::AsyncBufRead;

#[derive(Clone, Debug)]
pub enum RelevantEvent<'a> {
    /// Start tag (with attributes) `<tag attr="value">`.
    Start(BytesStart<'a>),
    /// End tag `</tag>`.
    End(BytesEnd<'a>),
    /// Empty element tag (with attributes) `<tag attr="value" />`.
    Empty(BytesStart<'a>),
    /// Character data between `Start` and `End` element.
    Text(String),
    /// End of XML document.
    Eof,
}

pub async fn read_relevant_event(
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
    buffer: &mut Vec<u8>,
) -> Result<RelevantEvent<'static>> {
    let relevant_event;

    loop {
        match reader.read_event_into_async(buffer).await?.into_owned() {
            Event::Start(tag) => {
                relevant_event = RelevantEvent::Start(tag);
                break;
            }
            Event::End(tag) => {
                relevant_event = RelevantEvent::End(tag);
                break;
            }
            Event::Empty(tag) => {
                relevant_event = RelevantEvent::Empty(tag);
                break;
            }
            Event::Text(text) => {
                if text.iter().any(|byte| !byte.is_ascii_whitespace()) {
                    let unescaped = text.unescape()?;
                    relevant_event = RelevantEvent::Text(unescaped.to_string());
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
