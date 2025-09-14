mod handler;

use handler::{http1_handle};


use std::collections::HashMap;
use std::fmt::Write;
use std::thread;
use std::io::{Read};
use std::str::FromStr;
use anyhow::Result;

use bytes::BytesMut;

use tokio::net::{TcpListener, TcpStream};
use tokio_util::io::read_buf;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;

enum HttpProtocol {
    HTTP1,
    HTTP2
}

async fn listen() -> Result<()>{
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let mut incoming = TcpListenerStream::new(listener);

    while let Some(Ok(tcp_stream)) = incoming.next().await {
        tokio::task::spawn(handle_request(tcp_stream));
    }

    Ok(())
}

// Tokio의 Single Thread Executor
async fn handle_request(mut stream: TcpStream) -> Result<()> {
    println!("Connection established! information {:#?}", stream);

    let (read_half, write_half) = stream.into_split();
    // TcpStream을 Reader / Writer 모두에게 소유권을 줄 수 없음.
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(write_half);

    let mut read_buf = Vec::with_capacity(1024);

    let client_protocol;
    loop {
        let read_size = reader.read_until(b'\n', &mut read_buf).await?;
        if read_size > 0 {
            client_protocol = verify_protocol(&read_buf);
            break;
        }
    }

    println!("client_protocol: {:?}", client_protocol);

    match client_protocol {
        VerifyProtocol::HTTP1 => {
            if let Ok(preface_message) = String::from_utf8(read_buf) {
                http1_handle(&preface_message, reader, writer).await?;
            }
            else {
                writer.write("Invalid HTTP1 preface message\n".to_string().as_bytes()).await?;
                writer.flush().await?;
            }
        }
        VerifyProtocol::HTTP2 => {

        }
        VerifyProtocol::INVALID => {
            writer.write("Invalid Protocol\n".to_string().as_bytes()).await?;
            writer.flush().await?;
        }
    }

    Ok(())
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

#[tokio::main]
async fn main() -> Result<()>{
    println!("Hello, world!");
    listen().await?;
    Ok(())
}
