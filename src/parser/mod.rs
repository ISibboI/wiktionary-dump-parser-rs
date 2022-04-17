use std::collections::HashSet;
use std::io::{BufRead, Read, Seek, Write};
use crate::error::Result;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::time::Duration;
use tokio::time::Instant;
use bzip2::bufread::MultiBzDecoder;
use log::{debug, info};
use quick_xml::Reader;
use tokio::io::{AsyncRead, ReadBuf};
use crate::Error;
use crate::parser::xml::{read_relevant_event, RelevantEvent};

mod xml;

struct TokioReadAdapter<R>(R);

impl<R: Read + Unpin> AsyncRead for TokioReadAdapter<R> {
    fn poll_read(mut self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let amount = self.0.read(buf.initialize_unfilled());
        Poll::Ready(amount.map(|amount| {
            buf.advance(amount);()
        }))
    }
}

pub async fn parse_dump_file(input_file: impl AsRef<Path>, output_file: impl AsRef<Path>) -> Result<()> {
    let input_file = input_file.as_ref();
    let output_file = output_file.as_ref();

    // TODO check how to do this better when we have internet again
    if input_file.extension().filter(|extension| extension.to_str() == Some("bz2")).is_some() {
        if input_file.file_stem().map(|stem| stem.to_str().filter(|stem| stem.ends_with("xml")).is_some()).is_none() {
            return Err(Error::Other(format!("Found a '.bz2' file extension that is not preceded by a '.xml' file extension in file {input_file:?}")));
        }

        debug!("Found file extension '.xml.bz2' for input file {input_file:?}");

        let input_file = std::fs::File::open(input_file)?;
        let input_size = input_file.metadata()?.len();
        let input_stream = std::io::BufReader::with_capacity(1024 * 1024, MultiBzDecoder::new(std::io::BufReader::new(input_file)));
        let output_stream = std::io::BufWriter::new(std::fs::File::create(output_file)?);

        // File is compressed, to input size is not accurate
        parse_dump_file_with_streams(input_stream, Box::new(move |input_stream| {
            (input_stream.get_ref().get_ref().get_ref().stream_position().map_err(Into::into), input_size)
        }), output_stream).await?;
    } else if input_file.extension().filter(|extension| extension.to_str() == Some("xml")).is_some() {
        debug!("Found file extension '.xml' for input file {input_file:?}");

        let input_file = std::fs::File::open(input_file)?;
        let input_size = input_file.metadata()?.len();
        let input_stream = std::io::BufReader::with_capacity(1024 * 1024, input_file);
        let output_stream = std::io::BufWriter::new(std::fs::File::create(output_file)?);
        parse_dump_file_with_streams(input_stream, Box::new(move |input_stream| {
            (input_stream.get_ref().stream_position().map_err(Into::into), input_size)
        }), output_stream).await?;
    } else {
        return Err(Error::Other(format!("Unknown file extension in file {input_file:?}")));
    }

    todo!()
}

async fn parse_dump_file_with_streams<InputStream: BufRead>(input_stream: InputStream, input_progress: Box<dyn Fn(&InputStream) -> (Result<u64>, u64)>, output_stream: impl Write) -> Result<()> {
    let mut reader = Reader::from_reader(input_stream);
    let mut buffer = Vec::new();
    let mut last_progress_log = Instant::now();
    let mut tag_stack = Vec::new();
    let mut tag_names = HashSet::new();
    let toplevel = 1;

    loop {
        let current_time = Instant::now();
        if current_time - last_progress_log >= Duration::from_secs(10) {
            last_progress_log = current_time;

            let (current, input_size) = input_progress(reader.underlying_reader_ref());
            let current = current?;
            let current_mib = current / (1024*1024);
            let input_size_mib = input_size / (1024*1024);

            info!("Parsing input file at {current_mib}/{input_size_mib}MiB");
        }

        match read_relevant_event(&mut reader, &mut buffer) {
            Ok(event) => match event {
                RelevantEvent::Start(tag) => {
                    if tag_stack.len() <= toplevel {
                        let level = tag_stack.len();
                        //debug!("Found level {level} tag {tag:?}");
                    }

                    tag_names.insert(String::from_utf8(tag.name().to_vec())?);
                    tag_stack.push(tag.into_owned());
                }
                RelevantEvent::End(_) => {tag_stack.pop();}
                RelevantEvent::Empty(tag) => {
                    if tag_stack.len() <= toplevel {
                        let level = tag_stack.len();
                        debug!("Found level {level} empty tag {tag:?}");
                    }
                    tag_names.insert(String::from_utf8(tag.name().to_vec())?);
                }
                RelevantEvent::Text(_) => {}
                RelevantEvent::Eof => {break;}
            },
            Err(e) => return Err(e.into()),
        }
    }

    info!("Found tag names {tag_names:?}");

    todo!()
}