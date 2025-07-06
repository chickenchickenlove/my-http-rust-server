use std::collections::HashMap;
use std::io::{Read};
use std::net::{TcpListener, TcpStream};
use std::str::FromStr;
use anyhow::Result;

enum HttpProtocol {
    HTTP1,
    HTTP2
}

fn listen() {
    let listener = TcpListener::bind("127.0.0.1:8080")
        .unwrap();

    for maybe_stream in listener.incoming() {
        if let Ok(mut stream) = maybe_stream {
            handle_request(&mut stream);
        }
        else {
            println!("Connection Failed");
        }
    }
}

fn handle_request(stream: &mut TcpStream) {
    println!("Connection established! information {:#?}", stream);

    loop {

        if let Ok(preface_buf) = read_until_preface_message(stream) {
            let protocol = verify_protocol(&preface_buf);
            read_header(stream);
        }

    }
}

fn verify_protocol(preface_message: &Vec<u8>) -> HttpProtocol {
    if preface_message == b"PRI * HTTP/2.0\r\n" {
        HttpProtocol::HTTP2
    }
    else {
        HttpProtocol::HTTP1
    }
}

// POST / HTTP/1.1\r\nHost: localhost:8080\r\nUser-Agent: curl/8.9.1\r\nAccept: */*\r\nContent-Length: 2290\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\n
fn read_until_preface_message(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut preface_buf = Vec::new();
    loop {
        let mut temp_buf = vec![0u8; 1];
        stream.read(&mut temp_buf)?;
        preface_buf.extend_from_slice(&temp_buf);

        if preface_buf.ends_with(b"\r\n") {
            break;
        }
    }

    Ok(preface_buf)
}

fn read_header(stream: &mut TcpStream) -> Result<Headers> {
    let mut headers_buf = Vec::new();

    loop {
        let mut temp_buf = vec![0u8; 1];
        stream.read(&mut temp_buf)?;
        headers_buf.extend_from_slice(&temp_buf);

        if headers_buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    if let Ok(headers) = Headers::from_str(String::from_utf8(headers_buf)?.as_str()) {
        println!("{:#?}", headers);
        Ok(headers)
    }
    else {
        Err(anyhow::Error::msg("Failed to parse headers"))
    }
}

#[derive(Debug)]
struct Headers {
    headers: HashMap<String, String>
}

#[derive(Debug)]
pub enum HeaderParseError {
    /// It is not `:` format.
    MissingColon,
    /// For example, "Host:"
    EmptyValue,
    // Etc
    InvalidFormat,
}


impl FromStr for Headers {
    type Err = HeaderParseError;

    fn from_str(headers_string: &str) -> std::result::Result<Self, Self::Err> {
        let mut headers: HashMap<String, String> = HashMap::new();
        for header_kv in headers_string.strip_suffix("\r\n\r\n").unwrap().split("\r\n") {
            if let Some((key, value)) = header_kv.split_once(": ") {
                headers.insert(key.to_string(), value.to_string());
            }
            else {
                return Err(HeaderParseError::InvalidFormat)
            }
        }

        Ok(Headers{headers: headers})
    }
}



fn main() {
    println!("Hello, world!");
    listen();
}
