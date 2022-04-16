use std::io::Read;
use tokio::fs::File;
use crate::error::Result;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use bzip2::bufread::MultiBzDecoder;
use log::{debug};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, BufWriter, ReadBuf};
use crate::Error;

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

    let input_filename = input_file.file_name().ok_or_else(|| Error::Other("File path contains no file name".to_owned()))?;

    // TODO check how to do this better when we have internet again
    if input_file.extension().filter(|extension| extension.to_str() == Some("bz2")).is_some() {
        if input_file.file_stem().map(|stem| stem.to_str().filter(|stem| stem.ends_with("xml")).is_some()).is_none() {
            return Err(Error::Other(format!("Found a '.bz2' file extension that is not preceded by a '.xml' file extension in file {input_file:?}")));
        }

        debug!("Found file extension '.xml.bz2' for input file {input_file:?}");

        let input_stream = tokio::io::BufReader::new(TokioReadAdapter(MultiBzDecoder::new(std::io::BufReader::new(std::fs::File::open(input_file)?))));
        let output_stream = BufWriter::new(File::create(output_file).await?);
        parse_dump_file_with_streams(input_stream, output_stream).await?;
    } else if input_file.extension().filter(|extension| extension.to_str() == Some("xml")).is_some() {
        debug!("Found file extension '.xml' for input file {input_file:?}");

        let input_stream = tokio::io::BufReader::new(File::open(input_file).await?);
        let output_stream = BufWriter::new(File::create(output_file).await?);
        parse_dump_file_with_streams(input_stream, output_stream).await?;
    } else {
        return Err(Error::Other(format!("Unknown file extension in file {input_file:?}")));
    }

    todo!()
}

async fn parse_dump_file_with_streams(mut input_stream: impl Unpin + AsyncRead, mut output_stream: impl AsyncWrite) -> Result<()> {
    let mut buffer = vec![0; 1024];
    let bytes_read = input_stream.read_exact(buffer.as_mut_slice()).await?;
    assert!(bytes_read <= buffer.len());
    buffer.resize(bytes_read, 0);

    // TODO assuming utf8 encoding here, but check if that is true
    let content = String::from_utf8(buffer)?;
    println!("{content}");

    todo!()
}