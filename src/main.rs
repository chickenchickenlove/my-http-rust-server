use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Read};
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

fn handle_request(stream: &TcpStream) {
    println!("Connection established! information {:#?}", stream);

    let mut reader = BufReader::new(stream);
    let mut writer = BufWriter::new(stream);

    loop {

        if let Ok(preface_message) = read_until_preface_message(&mut reader) {
            let protocol = verify_protocol(preface_message.as_str());
            read_header(&mut reader);
        }

    }
}

fn verify_protocol(preface_message: &str) -> HttpProtocol {
    if preface_message  == "PRI * HTTP/2.0\r\n" {
        HttpProtocol::HTTP2
    }
    else {
        HttpProtocol::HTTP1
    }
}

// POST / HTTP/1.1\r\nHost: localhost:8080\r\nUser-Agent: curl/8.9.1\r\nAccept: */*\r\nContent-Length: 2290\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\n
fn read_until_preface_message(reader: &mut BufReader<&TcpStream>) -> Result<String> {
    let mut preface_msg = String::new();
    loop {
        // TODO: Use Error to prevent panic.
        reader.read_line(&mut preface_msg);

        if preface_msg.ends_with("\r\n") {
            break;
        }
    }

    Ok(preface_msg)
}

fn read_header(reader: &mut BufReader<&TcpStream>) -> Result<Headers> {
    let mut headers_string = String::new();
    loop {
        reader.read_line(&mut headers_string);
        if headers_string.ends_with("\r\n\r\n") {
            break;
        }

    }

    if let Ok(headers) = Headers::from_str(headers_string.as_str()) {
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
