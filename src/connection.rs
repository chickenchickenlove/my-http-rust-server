use anyhow::{anyhow, bail, Result};
use chrono::{Utc};
use chrono::format::strftime::StrftimeItems;
use bytes::{BufMut, Bytes, BytesMut};
use tokio::net::{TcpStream};
use tokio::io::{
    AsyncBufReadExt,
    AsyncWriteExt,
    BufReader, BufWriter
};
use tokio::net::tcp::{
    OwnedReadHalf,
    OwnedWriteHalf};
use tracing::debug;
use crate::http_object::HttpResponse;
use crate::http_type::HttpProtocol;
use crate::connection_reader::{
    HttpConnectionContext1
};
use crate::http_connection_context::{
    Http11ConnectionContext1,
    Http1ConnectionContext1,
    Http2ConnectionContext1
};

pub struct ConnectionOwner {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
}

pub struct PrefaceMessage(pub Bytes);


impl PrefaceMessage {
    pub fn with(preface_message: impl Into<Bytes>) -> Self {
        PrefaceMessage { 0 : preface_message.into() }
    }
}


impl ConnectionOwner {

    // TODO : Buffer를 Cow로 처리하는게 낫지 않을까?
    pub fn new(tcp_stream: TcpStream) -> Self {
        // TcpStream을 Reader / Writer 모두에게 소유권을 줄 수 없음.
        let (read_half, write_half) = tcp_stream.into_split();

        let reader = BufReader::new(read_half);
        let writer = BufWriter::new(write_half);
        Self { reader, writer }
    }

    pub fn with(tcp_stream: TcpStream) -> Self {
        // TcpStream을 Reader / Writer 모두에게 소유권을 줄 수 없음.
        let (read_half, write_half) = tcp_stream.into_split();

        let reader = BufReader::new(read_half);
        let writer = BufWriter::new(write_half);
        Self { reader, writer }
    }

    pub fn into_parts(self) -> (BufReader<OwnedReadHalf>, BufWriter<OwnedWriteHalf>) {
        ( self.reader, self.writer )
    }

    pub fn reader(&mut self) -> &mut BufReader<OwnedReadHalf> {
        &mut self.reader
    }

    pub fn writer(&mut self) -> &mut BufWriter<OwnedWriteHalf> {
        &mut self.writer
    }

    pub async fn shutdown(mut self) -> () {
        if let Err(E) = self.writer.shutdown().await {
            debug!("failed to shutdown writer because of {}. maybe, it will be dropped.", E);
        }
    }

    pub async fn create_connection_context(&mut self) -> Result<(HttpConnectionContext1, PrefaceMessage)> {
        let mut read_buf = Vec::with_capacity(1024);

        let mut client_protocol = None;

        while let None = client_protocol {
            // read_until(...).await가 Ok(0)을 돌려주는 건 “느리다”가 아니라 진짜 EOF(연결이 닫힘)임.
            let read_size = self.reader.read_until(b'\n', &mut read_buf).await?;
            if read_size > 0 {
                client_protocol = verify_protocol(read_buf.as_slice());
            }
        }

        let connection_context: HttpConnectionContext1 = match client_protocol.unwrap() {
            HttpProtocol::HTTP1 => HttpConnectionContext1::HTTP1Context(Http1ConnectionContext1::new()),
            HttpProtocol::HTTP11 => HttpConnectionContext1::HTTP11Context(Http11ConnectionContext1::new()),
            HttpProtocol::HTTP2 => HttpConnectionContext1::HTTP2Context(Http2ConnectionContext1::new()),
            _ => bail!("Failed to"),
        };

        let preface_message = PrefaceMessage::with(read_buf);

        Ok((connection_context, preface_message))
    }

    // HTTP/1.1 200 OK
    // Date: Sun, 06 Nov 1994 08:49:37 GMT
    // Server: ExampleServer/0.1
    // Content-Type: text/plain; charset=utf-8
    // Content-Length: 14
    // Connection: keep-alive
    //
    // Hello, World!
    pub async fn response(&mut self, res: HttpResponse, should_close: bool) -> Result<()>{
        let mut writer_buffer = BytesMut::new();
        let (http_status, mut headers, mut maybe_body) = res.into_parts();

        let status_code: u16 = http_status.into();
        let status_text: String = http_status.into();
        let status_line = format!("{} {} {}\r\n", "HTTP/1.1", status_code, status_text);
        writer_buffer.put(status_line.as_bytes());

        if let Some(body) = maybe_body.as_ref() {
            headers.insert("Content-Length".to_string(), body.len().to_string());
        } else {
            headers.insert("Content-Length".to_string(), 0.to_string());
        }

        headers.insert("Connection".to_string(), (if should_close { "close" } else { "keep-alive" }).to_string());
        headers.insert("Server".to_string(), "My Rust Http Server/0.1".to_string());
        headers.insert("Date".to_string(),
                       Utc::now()
                           .format_with_items(StrftimeItems::new("%a, %d %b %Y %H:%M:%S GMT"))
                           .to_string()
        );

        let concat_headers = headers.into_iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<String>>()
            .join("\r\n");

        writer_buffer.put(concat_headers.as_bytes());
        writer_buffer.put("\r\n\r\n".as_bytes());

        if let Some(body) = maybe_body {
            writer_buffer.put(body);
        }

        // Don't call write(...) to prevent partial write.
        self.writer.write_all(writer_buffer.as_ref()).await?;
        self.writer.flush().await?;

        Ok(())
    }

}


fn verify_protocol(message: &[u8]) -> Option<HttpProtocol> {
    println!("message: {:?}", message);
    if message.starts_with(b"PRI * HTTP/2.0\r\n") {
        Some(HttpProtocol::HTTP2)
    }
    else if message.ends_with(b"HTTP/1.0\r\n") {
        Some(HttpProtocol::HTTP1)
    }
    else if message.ends_with(b"HTTP/1.1\r\n") {
        Some(HttpProtocol::HTTP11)
    }
    else {
        None
    }
}