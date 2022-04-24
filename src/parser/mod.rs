use crate::error::Result;
use crate::parser::xml::{read_relevant_event, RelevantEvent};
use crate::Error;
use bzip2::bufread::MultiBzDecoder;
use log::{debug, info};
use quick_xml::events::attributes::Attributes;
use quick_xml::Reader;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::io::{BufRead, Read, Seek, Write};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};
use tokio::time::Duration;
use tokio::time::Instant;

mod xml;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Siteinfo {
    sitename: String,
    dbname: String,
    base: String,
    generator: String,
    case: String,
    namespaces: Vec<Namespace>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Namespace {
    key: i32,
    case: String,
    name: String,
}

struct TokioReadAdapter<R>(R);

impl<R: Read + Unpin> AsyncRead for TokioReadAdapter<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let amount = self.0.read(buf.initialize_unfilled());
        Poll::Ready(amount.map(|amount| {
            buf.advance(amount);
        }))
    }
}

pub async fn parse_dump_file(
    input_file: impl AsRef<Path>,
    output_file: impl AsRef<Path>,
) -> Result<()> {
    let input_file = input_file.as_ref();
    let output_file = output_file.as_ref();

    // TODO check how to do this better when we have internet again
    if input_file
        .extension()
        .filter(|extension| extension.to_str() == Some("bz2"))
        .is_some()
    {
        if input_file
            .file_stem()
            .map(|stem| stem.to_str().filter(|stem| stem.ends_with("xml")).is_some())
            .is_none()
        {
            return Err(Error::Other(format!("Found a '.bz2' file extension that is not preceded by a '.xml' file extension in file {input_file:?}")));
        }

        debug!("Found file extension '.xml.bz2' for input file {input_file:?}");

        let input_file = std::fs::File::open(input_file)?;
        let input_size = input_file.metadata()?.len();
        let input_stream = std::io::BufReader::with_capacity(
            1024 * 1024,
            MultiBzDecoder::new(std::io::BufReader::new(input_file)),
        );
        let output_stream = std::io::BufWriter::new(std::fs::File::create(output_file)?);

        // File is compressed, to input size is not accurate
        parse_dump_file_with_streams(
            input_stream,
            Box::new(move |input_stream| {
                (
                    input_stream
                        .get_ref()
                        .get_ref()
                        .get_ref()
                        .stream_position()
                        .map_err(Into::into),
                    input_size,
                )
            }),
            output_stream,
        )
        .await?;
    } else if input_file
        .extension()
        .filter(|extension| extension.to_str() == Some("xml"))
        .is_some()
    {
        debug!("Found file extension '.xml' for input file {input_file:?}");

        let input_file = std::fs::File::open(input_file)?;
        let input_size = input_file.metadata()?.len();
        let input_stream = std::io::BufReader::with_capacity(1024 * 1024, input_file);
        let output_stream = std::io::BufWriter::new(std::fs::File::create(output_file)?);
        parse_dump_file_with_streams(
            input_stream,
            Box::new(move |input_stream| {
                (
                    input_stream.get_ref().stream_position().map_err(Into::into),
                    input_size,
                )
            }),
            output_stream,
        )
        .await?;
    } else {
        return Err(Error::Other(format!(
            "Unknown file extension in file {input_file:?}"
        )));
    }

    todo!()
}

async fn parse_dump_file_with_streams<InputStream: BufRead>(
    input_stream: InputStream,
    input_progress: Box<dyn Fn(&InputStream) -> (Result<u64>, u64)>,
    mut output_stream: impl Write,
) -> Result<()> {
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
            let current_mib = current / (1024 * 1024);
            let input_size_mib = input_size / (1024 * 1024);

            info!("Parsing input file at {current_mib}/{input_size_mib}MiB");
        }

        let level = tag_stack.len();
        match read_relevant_event(&mut reader, &mut buffer) {
            Ok(event) => {
                match event {
                    // level 1 tags: "siteinfo", "page"
                    RelevantEvent::Start(tag) => {
                        let tag_name = String::from_utf8(tag.name().to_vec())?;
                        if level == 0 {
                            if tag_name != "mediawiki" {
                                return Err(Error::Other(format!(
                                    "Found unexpected toplevel tag {tag:?}"
                                )));
                            }
                            tag_names.insert(tag_name.clone());
                            tag_stack.push(tag_name);
                        } else if level == 1 {
                            match tag_name.as_str() {
                                "siteinfo" => {
                                    let siteinfo =
                                        parse_siteinfo(tag.attributes(), &mut reader, &mut buffer)
                                            .await?;
                                    info!("{siteinfo:?}");
                                    serde_json::to_writer(&mut output_stream, &siteinfo)?;
                                }
                                "page" => {
                                    parse_page(tag.attributes(), &mut reader, &mut buffer).await?
                                }
                                _ => {
                                    return Err(Error::Other(format!(
                                        "Found unexpected level 1 tag {tag:?}"
                                    )))
                                }
                            }
                        }
                    }
                    RelevantEvent::End(tag) => {
                        let tag_name = String::from_utf8(tag.name().to_vec())?;
                        let stacked_tag = tag_stack.pop().ok_or_else(|| {
                            Error::Other(format!("Unexpected closing tag {tag:?}"))
                        })?;
                        if tag_name != stacked_tag {
                            return Err(Error::Other(format!("Unexpected closing tag {tag:?}")));
                        }
                    }
                    RelevantEvent::Empty(tag) => {
                        if level <= toplevel {
                            debug!("Found level {level} empty tag {tag:?}");
                            tag_names.insert(String::from_utf8(tag.name().to_vec())?);
                        }
                    }
                    RelevantEvent::Text(text) => {
                        return Err(Error::Other(format!("Unexpected text {text:?}")));
                    }
                    RelevantEvent::Eof => {
                        if level > 0 {
                            return Err(Error::Other(format!("Unexpected eof")));
                        } else {
                            break;
                        }
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }

    info!("Found tag names {tag_names:?}");

    todo!()
}

async fn parse_siteinfo<'attributes, InputStream: BufRead>(
    mut attributes: Attributes<'attributes>,
    reader: &mut Reader<InputStream>,
    buffer: &mut Vec<u8>,
) -> Result<Siteinfo> {
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    let mut sitename = None;
    let mut dbname = None;
    let mut base = None;
    let mut generator = None;
    let mut case = None;
    let mut namespaces = None;

    loop {
        match read_relevant_event(reader, buffer)? {
            RelevantEvent::Start(tag) => match tag.name() {
                b"sitename" => {
                    sitename =
                        Some(parse_string("sitename", tag.attributes(), reader, buffer).await?);
                }
                b"dbname" => {
                    dbname = Some(parse_string("dbname", tag.attributes(), reader, buffer).await?);
                }
                b"base" => {
                    base = Some(parse_string("base", tag.attributes(), reader, buffer).await?);
                }
                b"generator" => {
                    generator =
                        Some(parse_string("generator", tag.attributes(), reader, buffer).await?);
                }
                b"case" => {
                    case = Some(parse_string("case", tag.attributes(), reader, buffer).await?);
                }
                b"namespaces" => {
                    namespaces = Some(parse_namespaces(tag.attributes(), reader, buffer).await?);
                }
                _ => return Err(Error::Other(format!("Found unexpected tag {tag:?}"))),
            },
            RelevantEvent::End(tag) => {
                return if tag.name() == b"siteinfo" {
                    Ok(Siteinfo {
                        sitename: if let Some(sitename) = sitename {
                            sitename
                        } else {
                            return Err(Error::Other(format!("Missing sitename in siteinfo")));
                        },
                        dbname: if let Some(dbname) = dbname {
                            dbname
                        } else {
                            return Err(Error::Other(format!("Missing dbname in siteinfo")));
                        },
                        base: if let Some(base) = base {
                            base
                        } else {
                            return Err(Error::Other(format!("Missing base in siteinfo")));
                        },
                        generator: if let Some(generator) = generator {
                            generator
                        } else {
                            return Err(Error::Other(format!("Missing generator in siteinfo")));
                        },
                        case: if let Some(case) = case {
                            case
                        } else {
                            return Err(Error::Other(format!("Missing case in siteinfo")));
                        },
                        namespaces: if let Some(namespaces) = namespaces {
                            namespaces
                        } else {
                            return Err(Error::Other(format!("Missing namespaces in siteinfo")));
                        },
                    })
                } else {
                    Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )))
                };
            }
            RelevantEvent::Empty(tag) => {
                debug!("{tag:?}")
            }
            RelevantEvent::Text(text) => {
                debug!("{text:?}")
            }
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

async fn parse_namespaces<'attributes, InputStream: BufRead>(
    mut attributes: Attributes<'attributes>,
    reader: &mut Reader<InputStream>,
    buffer: &mut Vec<u8>,
) -> Result<Vec<Namespace>> {
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    struct NamespaceTag {
        key: i32,
        case: String,
    }
    let mut current_namespace_tag = None;
    let mut namespaces = Vec::new();

    loop {
        match read_relevant_event(reader, buffer)? {
            RelevantEvent::Start(tag) => {
                if tag.name() == b"namespace" {
                    if current_namespace_tag.is_some() {
                        return Err(Error::Other(format!("Found nested namespace tag {tag:?}")));
                    }

                    current_namespace_tag = Some(NamespaceTag {
                        key: String::from_utf8_lossy(
                            &tag.try_get_attribute(b"key")?
                                .ok_or_else(|| {
                                    Error::Other(format!("Missing attribute key in {tag:?}"))
                                })?
                                .value,
                        )
                        .parse()
                        .map_err(|_| Error::Other(format!("Key is not an integer in {tag:?}")))?,
                        case: String::from_utf8_lossy(
                            &tag.try_get_attribute(b"case")?
                                .ok_or_else(|| {
                                    Error::Other(format!("Missing attribute case in {tag:?}"))
                                })?
                                .value,
                        )
                        .into_owned(),
                    });
                } else {
                    return Err(Error::Other(format!("Found unexpected tag {tag:?}")));
                }
            }
            RelevantEvent::End(tag) => {
                if tag.name() == b"namespaces" {
                    return Ok(namespaces);
                } else if tag.name() == b"namespace" {
                    if current_namespace_tag.is_some() {
                        return Err(Error::Other(format!(
                            "Found namespace tag without text {tag:?}"
                        )));
                    }
                } else {
                    return Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )));
                };
            }
            RelevantEvent::Empty(tag) => {
                debug!("{tag:?}")
            }
            RelevantEvent::Text(text) => {
                if let Some(current_namespace_tag) = current_namespace_tag {
                    namespaces.push(Namespace {
                        key: current_namespace_tag.key,
                        case: current_namespace_tag.case,
                        name: String::from_utf8_lossy(&text).into_owned(),
                    });
                } else {
                    return Err(Error::Other(format!(
                        "Found text outside of namespace tag: {text:?}"
                    )));
                }

                current_namespace_tag = None;
            }
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

async fn parse_string<'attributes, InputStream: BufRead>(
    name: impl AsRef<[u8]>,
    mut attributes: Attributes<'attributes>,
    reader: &mut Reader<InputStream>,
    buffer: &mut Vec<u8>,
) -> Result<String> {
    let name = name.as_ref();
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    let mut value = String::new();

    loop {
        match read_relevant_event(reader, buffer)? {
            RelevantEvent::Start(tag) => {
                return Err(Error::Other(format!("Found unexpected tag {tag:?}")));
            }
            RelevantEvent::End(tag) => {
                return if tag.name() == name {
                    Ok(value)
                } else {
                    Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )))
                };
            }
            RelevantEvent::Empty(tag) => {
                debug!("{tag:?}")
            }
            RelevantEvent::Text(text) => value = String::from_utf8(text.to_vec())?,
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

async fn parse_page<'attributes, InputStream: BufRead>(
    mut attributes: Attributes<'attributes>,
    reader: &mut Reader<InputStream>,
    buffer: &mut Vec<u8>,
) -> Result<()> {
    todo!("parse_page")
}
