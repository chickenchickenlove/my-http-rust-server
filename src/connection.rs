// mod handler;
// mod ash;
// mod dispatcher;
// mod connection;
//
// use handler::{http1_handle};

use std::fmt::format;
use std::io::{Read};
use std::str::FromStr;
use anyhow::{bail, Result};
use crate::handler::{Http1Handler, HttpConnectionHandler, HttpConnectionContext};

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use crate::http_object::HttpResponse;

pub struct ConnectionOwner {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
}

impl ConnectionOwner {

    pub fn new(tcp_stream: TcpStream) -> Self {
        // TcpStream을 Reader / Writer 모두에게 소유권을 줄 수 없음.
        let (read_half, write_half) = tcp_stream.into_split();

        let reader = BufReader::new(read_half);
        let writer = BufWriter::new(write_half);
        Self { reader, writer }
    }

    pub async fn handle(&mut self) -> Result<HttpConnectionContext> {
        // println!("Connection established! information {:#?}", self.tcp_stream);

        let mut read_buf = Vec::with_capacity(1024);

        let client_protocol;
        loop {
            let read_size = self.reader.read_until(b'\n', &mut read_buf).await?;
            if read_size > 0 {
                client_protocol = verify_protocol(&read_buf);
                break;
            }
        }

        println!("client_protocol: {:?}", client_protocol);

        let conn_context = match client_protocol {
            VerifyProtocol::HTTP1 => {
                if let Ok(preface_message) = String::from_utf8(read_buf) {
                    Http1Handler::new()
                        .handle(preface_message.as_str(),
                                &mut self.reader,
                                &mut self.writer,
                                )
                        .await?
                }
                else {
                    self.writer.write("Invalid HTTP1 preface message\n".to_string().as_bytes()).await?;
                    self.writer.flush().await?;
                    bail!("Invalid HTTP1 preface message\n")
                }
            }
            VerifyProtocol::HTTP2 => {
                self.writer.write("Currently HTTP/2 is unsupported.\n".to_string().as_bytes()).await?;
                self.writer.flush().await?;
                bail!("Currently HTTP/2 is unsupported.\n")
            }
            VerifyProtocol::INVALID => {
                self.writer.write("Invalid Protocol\n".to_string().as_bytes()).await?;
                self.writer.flush().await?;
                bail!("Unsupported Protocol or Invalid protocol")
            }
        };

        Ok(conn_context)
    }

    // HTTP/1.1 200 OK
    // Date: Sun, 06 Nov 1994 08:49:37 GMT
    // Server: ExampleServer/0.1
    // Content-Type: text/plain; charset=utf-8
    // Content-Length: 14
    // Connection: keep-alive
    //
    // Hello, World!
    pub async fn response(&mut self, res: HttpResponse) {
        let status_code: u16 = res.get_status_code().into();
        let status_text: String = res.get_status_code().into();

        let header = "Content-Length: 0\r\n";

        let status_line = format!("{} {} {}\r\n", "HTTP/1.1", status_code, status_text);
        let total = format!("{}{}\r\n", status_line, header);

        println!("ASH {}", total);
        self.writer.write(total.as_bytes()).await;
        self.writer.flush().await;

    }

}





#[derive(Debug)]
enum VerifyProtocol {
    HTTP1,
    HTTP2,
    INVALID,
}

fn verify_protocol(message: &Vec<u8>) -> VerifyProtocol {
    println!("{:?}", message);
    if message.starts_with(b"PRI * HTTP/2.0") {
        VerifyProtocol::HTTP2
    }
    else if message.ends_with(b"HTTP/1\r\n") {
        VerifyProtocol::HTTP1
    }
    else if message.ends_with(b"HTTP/1.1\r\n") {
        VerifyProtocol::HTTP1
    }
    else {
        VerifyProtocol::INVALID
    }
}