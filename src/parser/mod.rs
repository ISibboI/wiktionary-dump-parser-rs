use crate::error::Result;
use crate::parser::words::wikitext_to_words;
use crate::parser::xml::{read_relevant_event, RelevantEvent};
use crate::Error;
use async_compression::tokio::bufread::BzDecoder;
use log::{debug, info, trace, warn};
use quick_xml::events::attributes::Attributes;
use quick_xml::name::QName;
use quick_xml::Reader;
use serde::Deserialize;
use serde::Serialize;
use std::ffi::OsStr;
use std::future::Future;
use std::io::{Read, Write};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::File;
use tokio::io::{
    AsyncBufRead, AsyncRead, AsyncSeekExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter, ReadBuf,
};
use tokio::time::Duration;
use tokio::time::Instant;
use wikitext_parser::{parse_wikitext, Wikitext};

use self::words::Word;

pub mod words;
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
    key: i64,
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

pub async fn parse_dump_file<
    WordConsumerResult: Future<Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>>,
>(
    input_file: impl AsRef<Path>,
    output_file: Option<impl AsRef<Path>>,
    mut word_consumer: impl FnMut(Word) -> WordConsumerResult,
    error_log: impl AsRef<Path>,
    output_pretty: bool,
) -> Result<()> {
    let input_file = input_file.as_ref();
    let output_file = output_file.as_ref();

    if input_file.extension().map(OsStr::to_str) == Some(Some("bz2")) {
        if input_file
            .file_stem()
            .map(|stem| stem.to_str().filter(|stem| stem.ends_with("xml")).is_some())
            .is_none()
        {
            return Err(Error::Other(format!("Found a '.bz2' file extension that is not preceded by a '.xml' file extension in file {input_file:?}")));
        }

        debug!("Found file extension '.xml.bz2' for input file {input_file:?}");

        let input_file = File::open(input_file).await?;
        let input_size = input_file.metadata().await?.len();
        let input_stream = BufReader::with_capacity(
            1024 * 1024,
            BzDecoder::new(BufReader::with_capacity(1024 * 1024, input_file)),
        );

        let output_stream = if let Some(output_file) = output_file {
            Some(BufWriter::with_capacity(
                1024 * 1024,
                File::create(output_file).await?,
            ))
        } else {
            None
        };
        let error_log = std::io::BufWriter::new(std::fs::File::create(error_log)?);

        // File is compressed, so input size is not accurate
        parse_dump_file_with_streams(
            input_stream,
            |input_stream| input_stream.get_mut().get_mut().get_mut(),
            input_size,
            output_stream,
            &mut word_consumer,
            error_log,
            output_pretty,
        )
        .await?;
    } else if input_file
        .extension()
        .filter(|extension| extension.to_str() == Some("xml"))
        .is_some()
    {
        debug!("Found file extension '.xml' for input file {input_file:?}");

        let input_file = File::open(input_file).await?;
        let input_size = input_file.metadata().await?.len();
        let input_stream = BufReader::with_capacity(1024 * 1024, input_file);
        let output_stream = if let Some(output_file) = output_file {
            Some(BufWriter::with_capacity(
                1024 * 1024,
                File::create(output_file).await?,
            ))
        } else {
            None
        };
        let error_log = std::io::BufWriter::new(std::fs::File::create(error_log)?);

        parse_dump_file_with_streams(
            input_stream,
            |input_stream| input_stream.get_mut(),
            input_size,
            output_stream,
            &mut word_consumer,
            error_log,
            output_pretty,
        )
        .await?;
    } else {
        return Err(Error::Other(format!(
            "Unknown file extension in file {input_file:?}"
        )));
    }

    Ok(())
}

#[allow(clippy::type_complexity)]
async fn parse_dump_file_with_streams<
    InputStream: AsyncBufRead + Unpin,
    WordConsumerResult: Future<Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>>,
>(
    input_stream: InputStream,
    input_stream_to_file: impl Fn(&mut InputStream) -> &mut File,
    input_size: u64,
    mut output_stream: Option<impl AsyncWrite + Unpin>,
    word_consumer: &mut impl FnMut(Word) -> WordConsumerResult,
    mut error_log: impl Write,
    output_pretty: bool,
) -> Result<()> {
    let mut reader = Reader::from_reader(input_stream);
    let mut buffer = Vec::new();
    let mut last_progress_log = Instant::now();
    let mut tag_stack = Vec::new();
    let mut json_buffer = Vec::new();

    loop {
        let current_time = Instant::now();
        if current_time - last_progress_log >= Duration::from_secs(10) {
            last_progress_log = current_time;

            let input_file = input_stream_to_file(reader.get_mut());
            let current = input_file.stream_position().await?;
            let current_mib = current / (1024 * 1024);
            let input_size_mib = input_size / (1024 * 1024);

            info!("Parsing input file at {current_mib}/{input_size_mib}MiB");
        }

        let level = tag_stack.len();
        match read_relevant_event(&mut reader, &mut buffer).await {
            Ok(event) => match event {
                RelevantEvent::Start(tag) => {
                    let tag_name = String::from_utf8(tag.name().into_inner().to_vec())?;
                    if level == 0 {
                        if tag_name != "mediawiki" {
                            return Err(Error::Other(format!(
                                "Found unexpected toplevel tag {tag:?}"
                            )));
                        }
                        tag_stack.push(tag_name);
                    } else if level == 1 {
                        match tag_name.as_str() {
                            "siteinfo" => {
                                let siteinfo =
                                    parse_siteinfo(tag.attributes(), &mut reader, &mut buffer)
                                        .await?;
                                info!(
                                    "{} ({} {})",
                                    siteinfo.sitename, siteinfo.dbname, siteinfo.generator
                                );
                                if let Some(output_stream) = output_stream.as_mut() {
                                    json_buffer.clear();
                                    if output_pretty {
                                        serde_json::to_writer_pretty(&mut json_buffer, &siteinfo)?;
                                    } else {
                                        serde_json::to_writer(&mut json_buffer, &siteinfo)?;
                                    }
                                    output_stream.write_all(&json_buffer).await?;
                                }
                            }
                            "page" => {
                                let page = parse_page(
                                    tag.attributes(),
                                    &mut reader,
                                    word_consumer,
                                    &mut buffer,
                                    &mut error_log,
                                )
                                .await?;
                                trace!("{page:?}");
                                if let Some(output_stream) = output_stream.as_mut() {
                                    json_buffer.clear();
                                    if output_pretty {
                                        serde_json::to_writer_pretty(&mut json_buffer, &page)?;
                                    } else {
                                        serde_json::to_writer(&mut json_buffer, &page)?;
                                    }
                                    output_stream.write_all(&json_buffer).await?;
                                }
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
                    let tag_name = String::from_utf8(tag.name().into_inner().to_vec())?;
                    let stacked_tag = tag_stack
                        .pop()
                        .ok_or_else(|| Error::Other(format!("Unexpected closing tag {tag:?}")))?;
                    if tag_name != stacked_tag {
                        return Err(Error::Other(format!("Unexpected closing tag {tag:?}")));
                    }
                }
                RelevantEvent::Empty(tag) => {
                    return Err(Error::Other(format!("Unexpected empty tag {tag:?}")));
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
            },
            Err(error) => return Err(error),
        }
    }

    info!("Successfully parsed dump file");
    Ok(())
}

async fn parse_siteinfo(
    mut attributes: Attributes<'_>,
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
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
        match read_relevant_event(reader, buffer).await? {
            RelevantEvent::Start(tag) => match tag.name().into_inner() {
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
                return if tag.name() == QName(b"siteinfo") {
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
                warn!("{tag:?}")
            }
            RelevantEvent::Text(text) => {
                warn!("{text:?}")
            }
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

async fn parse_namespaces(
    mut attributes: Attributes<'_>,
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
    buffer: &mut Vec<u8>,
) -> Result<Vec<Namespace>> {
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    struct NamespaceTag {
        key: i64,
        case: String,
    }
    let mut current_namespace_tag = None;
    let mut namespaces = Vec::new();

    loop {
        match read_relevant_event(reader, buffer).await? {
            RelevantEvent::Start(tag) => {
                if tag.name() == QName(b"namespace") {
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
                if tag.name() == QName(b"namespaces") {
                    return Ok(namespaces);
                } else if tag.name() == QName(b"namespace") {
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
                match tag.name().into_inner() {
                    b"namespace" => { /* ignore nameless namespace */ }
                    _ => warn!("{tag:?}"),
                }
            }
            RelevantEvent::Text(text) => {
                if let Some(current_namespace_tag) = current_namespace_tag {
                    namespaces.push(Namespace {
                        key: current_namespace_tag.key,
                        case: current_namespace_tag.case,
                        name: text,
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

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Page {
    title: String,
    namespace: i64,
    id: i64,
    revision: Revision,
    redirect: Option<String>,
}

async fn parse_page<
    WordConsumerResult: Future<Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>>,
>(
    mut attributes: Attributes<'_>,
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
    word_consumer: &mut impl FnMut(Word) -> WordConsumerResult,
    buffer: &mut Vec<u8>,
    error_log: &mut impl Write,
) -> Result<Page> {
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    let mut title = None;
    let mut namespace = None;
    let mut id = None;
    let mut revision = None;
    let mut redirect = None;

    loop {
        match read_relevant_event(reader, buffer).await? {
            RelevantEvent::Start(tag) => match tag.name().into_inner() {
                b"title" => {
                    title = Some(parse_string("title", tag.attributes(), reader, buffer).await?);
                }
                b"ns" => {
                    namespace = Some(
                        parse_string("ns", tag.attributes(), reader, buffer)
                            .await?
                            .parse()
                            .map_err(|_| {
                                Error::Other(format!("ns is not an integer in {tag:?}"))
                            })?,
                    );
                }
                b"id" => {
                    id = Some(
                        parse_string("id", tag.attributes(), reader, buffer)
                            .await?
                            .parse()
                            .map_err(|_| {
                                Error::Other(format!("id is not an integer in {tag:?}"))
                            })?,
                    );
                }
                b"revision" => {
                    revision = Some(
                        parse_revision(
                            tag.attributes(),
                            title.clone(),
                            reader,
                            word_consumer,
                            buffer,
                            error_log,
                        )
                        .await?,
                    );
                }
                _ => return Err(Error::Other(format!("Found unexpected tag {tag:?}"))),
            },
            RelevantEvent::End(tag) => {
                return if tag.name() == QName(b"page") {
                    Ok(Page {
                        title: if let Some(title) = title {
                            title
                        } else {
                            return Err(Error::Other(format!("Missing title in page")));
                        },
                        namespace: if let Some(namespace) = namespace {
                            namespace
                        } else {
                            return Err(Error::Other(format!("Missing namespace in page")));
                        },
                        id: if let Some(id) = id {
                            id
                        } else {
                            return Err(Error::Other(format!("Missing id in page")));
                        },
                        revision: if let Some(revision) = revision {
                            revision
                        } else {
                            return Err(Error::Other(format!("Missing revision in page")));
                        },
                        redirect,
                    })
                } else {
                    Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )))
                };
            }
            RelevantEvent::Empty(tag) => match tag.name().into_inner() {
                b"redirect" => {
                    for attribute in tag.attributes() {
                        let attribute = attribute?;
                        match attribute.key {
                            QName(b"title") => {
                                redirect = Some(String::from_utf8(attribute.value.to_vec())?);
                            }
                            _ => warn!("{tag:?} {attribute:?}"),
                        }
                    }
                }
                _ => warn!("{tag:?}"),
            },
            RelevantEvent::Text(text) => {
                warn!("{text:?}")
            }
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Revision {
    id: i64,
    parentid: Option<i64>,
    timestamp: String,
    contributor: Option<Contributor>,
    comment: Option<String>,
    model: String,
    format: String,
    text: Option<Text>,
    sha1: String,
    minor: bool,
}

async fn parse_revision<
    WordConsumerResult: Future<Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>>,
>(
    mut attributes: Attributes<'_>,
    title: Option<String>,
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
    word_consumer: &mut impl FnMut(Word) -> WordConsumerResult,
    buffer: &mut Vec<u8>,
    error_log: &mut impl Write,
) -> Result<Revision> {
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    let mut id = None;
    let mut parentid = None;
    let mut timestamp = None;
    let mut contributor = None;
    let mut comment = None;
    let mut model = None;
    let mut format = None;
    let mut text = None;
    let mut sha1 = None;
    let mut minor = false;

    loop {
        match read_relevant_event(reader, buffer).await? {
            RelevantEvent::Start(tag) => match tag.name().into_inner() {
                b"id" => {
                    id = Some(
                        parse_string("id", tag.attributes(), reader, buffer)
                            .await?
                            .parse()
                            .map_err(|_| {
                                Error::Other(format!("id is not an integer in {tag:?}"))
                            })?,
                    );
                }
                b"parentid" => {
                    parentid = Some(
                        parse_string("parentid", tag.attributes(), reader, buffer)
                            .await?
                            .parse()
                            .map_err(|_| {
                                Error::Other(format!("parentid is not an integer in {tag:?}"))
                            })?,
                    );
                }
                b"timestamp" => {
                    timestamp =
                        Some(parse_string("timestamp", tag.attributes(), reader, buffer).await?);
                }
                b"contributor" => {
                    contributor = Some(parse_contributor(tag.attributes(), reader, buffer).await?);
                }
                b"comment" => {
                    comment =
                        Some(parse_string("comment", tag.attributes(), reader, buffer).await?);
                }
                b"model" => {
                    model = Some(parse_string("model", tag.attributes(), reader, buffer).await?);
                }
                b"format" => {
                    format = Some(parse_string("format", tag.attributes(), reader, buffer).await?);
                }
                b"text" => {
                    text = Some(
                        parse_text(
                            tag.attributes(),
                            title.as_deref(),
                            reader,
                            word_consumer,
                            buffer,
                            error_log,
                        )
                        .await?,
                    );
                }
                b"sha1" => {
                    sha1 = Some(parse_string("sha1", tag.attributes(), reader, buffer).await?);
                }
                _ => return Err(Error::Other(format!("Found unexpected tag {tag:?}"))),
            },
            RelevantEvent::End(tag) => {
                return if tag.name() == QName(b"revision") {
                    if text.is_none() {
                        debug!("No text for revision with id {id:?} and comment {comment:?}");
                    }

                    Ok(Revision {
                        id: if let Some(id) = id {
                            id
                        } else {
                            return Err(Error::Other(format!("Missing id in revision")));
                        },
                        parentid,
                        timestamp: if let Some(timestamp) = timestamp {
                            timestamp
                        } else {
                            return Err(Error::Other(format!("Missing timestamp in revision")));
                        },
                        contributor,
                        comment,
                        model: if let Some(model) = model {
                            model
                        } else {
                            return Err(Error::Other(format!("Missing model in revision")));
                        },
                        format: if let Some(format) = format {
                            format
                        } else {
                            return Err(Error::Other(format!("Missing format in revision")));
                        },
                        text,
                        sha1: if let Some(sha1) = sha1 {
                            sha1
                        } else {
                            return Err(Error::Other(format!("Missing sha1 in revision")));
                        },
                        minor,
                    })
                } else {
                    Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )))
                };
            }
            RelevantEvent::Empty(tag) => {
                match tag.name().into_inner() {
                    b"minor" => {
                        minor = true;
                    }
                    b"comment" => { /* ignore empty comment */ }
                    b"text" => { /* ignore empty text */ }
                    b"contributor" => { /* ignore empty contributor */ }
                    _ => warn!("{tag:?}"),
                }
            }
            RelevantEvent::Text(text) => {
                warn!("{text:?}")
            }
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum Contributor {
    User { username: String, id: i64 },
    Anonymous { ip: String },
}

async fn parse_contributor(
    mut attributes: Attributes<'_>,
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
    buffer: &mut Vec<u8>,
) -> Result<Contributor> {
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    let mut username = None;
    let mut id: Option<i64> = None;
    let mut ip = None;

    loop {
        match read_relevant_event(reader, buffer).await? {
            RelevantEvent::Start(tag) => match tag.name().into_inner() {
                b"username" => {
                    username =
                        Some(parse_string("username", tag.attributes(), reader, buffer).await?);
                }
                b"id" => {
                    id = Some(
                        parse_string("id", tag.attributes(), reader, buffer)
                            .await?
                            .parse()
                            .map_err(|_| {
                                Error::Other(format!("id is not an integer in {tag:?}"))
                            })?,
                    );
                }
                b"ip" => {
                    ip = Some(parse_string("ip", tag.attributes(), reader, buffer).await?);
                }
                _ => return Err(Error::Other(format!("Found unexpected tag {tag:?}"))),
            },
            RelevantEvent::End(tag) => {
                return if tag.name() == QName(b"contributor") {
                    if let (Some(username), Some(id), None) = (&username, &id, &ip) {
                        Ok(Contributor::User {
                            username: username.clone(),
                            id: *id,
                        })
                    } else if let (None, None, Some(ip)) = (&username, &id, &ip) {
                        Ok(Contributor::Anonymous { ip: ip.clone() })
                    } else {
                        Err(Error::Other(format!("Unknown combination of fields for contributor: {username:?}, {id:?}, {ip:?}")))
                    }
                } else {
                    Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )))
                };
            }
            RelevantEvent::Empty(tag) => {
                warn!("{tag:?}")
            }
            RelevantEvent::Text(text) => {
                warn!("{text:?}")
            }
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Text {
    xml_space: XmlSpace,
    text: Wikitext,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum XmlSpace {
    Preserve,
}

async fn parse_text<
    WordConsumerResult: Future<Output = std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>>,
>(
    attributes: Attributes<'_>,
    title: Option<&str>,
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
    mut word_consumer: &mut impl FnMut(Word) -> WordConsumerResult,
    buffer: &mut Vec<u8>,
    error_log: &mut impl Write,
) -> Result<Text> {
    let mut bytes: Option<usize> = None;
    let mut xml_space = None;

    for attribute in attributes {
        let attribute = attribute?;
        match attribute.key.into_inner() {
            b"bytes" => {
                bytes = Some(
                    String::from_utf8(attribute.value.to_vec())?
                        .parse()
                        .map_err(|_| {
                            Error::Other(format!("bytes is not an integer in {attribute:?}"))
                        })?,
                );
            }
            b"xml:space" => {
                xml_space = Some(match attribute.value.as_ref() {
                    b"preserve" => XmlSpace::Preserve,
                    _ => {
                        return Err(Error::Other(format!(
                            "Found unexpected attribute value {attribute:?}"
                        )))
                    }
                });
            }
            _ => {
                return Err(Error::Other(format!(
                    "Found unexpected attribute {attribute:?}"
                )))
            }
        }
    }

    let mut text = None;

    loop {
        match read_relevant_event(reader, buffer).await? {
            RelevantEvent::Start(tag) => {
                return Err(Error::Other(format!("Found unexpected tag {tag:?}")));
            }
            RelevantEvent::End(tag) => {
                return if tag.name() == QName(b"text") {
                    Ok(Text {
                        xml_space: if let Some(xml_space) = xml_space {
                            xml_space
                        } else {
                            return Err(Error::Other(format!("Missing tag xml:space in text")));
                        },
                        text: if let Some(text) = text {
                            text
                        } else {
                            return Err(Error::Other(format!("Missing text in text")));
                        },
                    })
                } else {
                    Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )))
                };
            }
            RelevantEvent::Empty(tag) => {
                warn!("{tag:?}")
            }
            RelevantEvent::Text(raw_text) => {
                if let Some(bytes) = bytes {
                    let raw_text_len = raw_text.len();
                    if raw_text_len != bytes {
                        warn!("Text length mismatch, attribute states {bytes}, but we got {raw_text_len}");
                    }
                }
                assert!(text.is_none());
                if title.is_none() {
                    warn!("Page content is parsed before its title.");
                }

                debug!("Parsing '{}'", title.unwrap_or("<unknown>"));
                let mut parser_errors = Vec::new();
                let parsed_text = parse_wikitext(
                    &raw_text,
                    title.map(ToString::to_string).unwrap_or_default(),
                    |error| parser_errors.push(error),
                );

                let page_name = title.map(ToString::to_string).unwrap_or_default();

                let mut word_errors = Vec::new();
                wikitext_to_words(&page_name, &parsed_text, &mut word_consumer, |error| {
                    word_errors.push(error)
                })
                .await?;

                if !parser_errors.is_empty() || !word_errors.is_empty() {
                    debug!("Page '{page_name}' has {} errors", parser_errors.len());
                    writeln!(error_log, "Page: {page_name}")
                        .unwrap_or_else(|error| panic!("Writing to error log failed: {error}"));
                }
                for error in &parser_errors {
                    writeln!(error_log, "{error:#?}")
                        .unwrap_or_else(|error| panic!("Writing to error log failed: {error}"));
                }
                for error in &word_errors {
                    writeln!(error_log, "{error:#?}")
                        .unwrap_or_else(|error| panic!("Writing to error log failed: {error}"));
                }
                if !parser_errors.is_empty() || !word_errors.is_empty() {
                    writeln!(error_log, "\nContent: {raw_text}\n")
                        .unwrap_or_else(|error| panic!("Writing to error log failed: {error}"));
                }

                text = Some(parsed_text);
            }
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}

async fn parse_string(
    name: impl AsRef<[u8]>,
    mut attributes: Attributes<'_>,
    reader: &mut Reader<impl AsyncBufRead + Unpin>,
    buffer: &mut Vec<u8>,
) -> Result<String> {
    let name = name.as_ref();
    if let Some(attribute) = attributes.next() {
        return Err(Error::Other(format!("Unexpected attribute {attribute:?}")));
    }

    let mut value = String::new();

    loop {
        match read_relevant_event(reader, buffer).await? {
            RelevantEvent::Start(tag) => {
                return Err(Error::Other(format!("Found unexpected tag {tag:?}")));
            }
            RelevantEvent::End(tag) => {
                return if tag.name() == QName(name) {
                    Ok(value)
                } else {
                    Err(Error::Other(format!(
                        "Found unexpected closing tag {tag:?}"
                    )))
                };
            }
            RelevantEvent::Empty(tag) => {
                warn!("{tag:?}")
            }
            RelevantEvent::Text(text) => value = text,
            RelevantEvent::Eof => return Err(Error::Other(format!("Unexpected eof"))),
        }
    }
}
