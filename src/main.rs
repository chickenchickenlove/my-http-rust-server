use std::collections::HashMap;
use std::fmt::Write;
use std::thread;
use std::io::{BufRead, BufReader, BufWriter, Read};
// use std::net::{TcpListener, TcpStream};
use std::str::FromStr;
use anyhow::Result;

use bytes::BytesMut;

use tokio::net::{TcpListener, TcpStream};
use tokio_util::io::read_buf;
use tokio::io::{AsyncRead, AsyncWrite};
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

    let mut buf = BytesMut::with_capacity(1024);
    read_buf(&mut stream, &mut buf).await?;

    let client_protocol;

    loop {
        println!("Parse Protocol");
        let mut lines = buf.lines();
        if let Some(Ok(line)) = lines.next() {
            client_protocol = verify_protocol(&line);
            break
        }
        else {
            read_buf(&mut stream, &mut buf).await?;
        }
    }

    println!("client_protocol: {:?}", client_protocol);

    Ok(())
}

#[derive(Debug)]
enum VerifyProtocol {
    HTTP1,
    HTTP2,
    INVALID,
}

fn verify_protocol(message: &String) -> VerifyProtocol {
    if message.starts_with("PRI * HTTP/2.0") {
        println!("here1");
        VerifyProtocol::HTTP2
    }
    else {
        println!("message : {}", message);
        if message.contains("HTTP/1") {
            VerifyProtocol::HTTP1
        }
        else {
            VerifyProtocol::INVALID
        }
    }
}


// fn verify_protocol(preface_message: &str) -> HttpProtocol {
//     if preface_message  == "PRI * HTTP/2.0\r\n" {
//         HttpProtocol::HTTP2
//     }
//     else {
//         HttpProtocol::HTTP1
//     }
// }
//
// // POST / HTTP/1.1\r\nHost: localhost:8080\r\nUser-Agent: curl/8.9.1\r\nAccept: */*\r\nContent-Length: 2290\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\n
// fn read_until_preface_message(reader: &mut BufReader<&TcpStream>) -> Result<String> {
//     let mut preface_msg = String::new();
//     loop {
//         // TODO: Use Error to prevent panic.
//         // TODO : async.
//         reader.read_line(&mut preface_msg);
//
//         if preface_msg.ends_with("\r\n") {
//             break;
//         }
//     }
//
//     Ok(preface_msg)
// }
//
// fn read_header(reader: &mut BufReader<&TcpStream>) -> Result<Headers> {
//     let mut headers_string = String::new();
//     loop {
//         reader.read_line(&mut headers_string);
//         if headers_string.ends_with("\r\n\r\n") {
//             break;
//         }
//
//     }
//
//     if let Ok(headers) = Headers::from_str(headers_string.as_str()) {
//         println!("{:#?}", headers);
//         Ok(headers)
//     }
//     else {
//         Err(anyhow::Error::msg("Failed to parse headers"))
//     }
// }
//
// #[derive(Debug)]
// struct Headers {
//     headers: HashMap<String, String>
// }
//
// #[derive(Debug)]
// pub enum HeaderParseError {
//     /// It is not `:` format.
//     MissingColon,
//     /// For example, "Host:"
//     EmptyValue,
//     // Etc
//     InvalidFormat,
// }
//
//
// impl FromStr for Headers {
//     type Err = HeaderParseError;
//
//     fn from_str(headers_string: &str) -> std::result::Result<Self, Self::Err> {
//         let mut headers: HashMap<String, String> = HashMap::new();
//         for header_kv in headers_string.strip_suffix("\r\n\r\n").unwrap().split("\r\n") {
//             if let Some((key, value)) = header_kv.split_once(": ") {
//                 headers.insert(key.to_string(), value.to_string());
//             }
//             else {
//                 return Err(HeaderParseError::InvalidFormat)
//             }
//         }
//
//         Ok(Headers{headers: headers})
//     }
// }

#[tokio::main]
async fn main() -> Result<()>{
    println!("Hello, world!");
    listen().await?;
    Ok(())
}
