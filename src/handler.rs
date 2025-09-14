use std::collections::HashMap;
use anyhow::{Result, Error, bail};
use tokio::io::{split, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};


struct Http1ConnectionContext {}
struct Http1RequestContext {
    method: String,
    path: String,
    protocol: String,
    headers: HashMap<String, String>,
    body: String
}

// POST /api/v1/widgets?limit=10 HTTP/1.1
// Host: example.com
// User-Agent: telnet
// Accept: application/json
// Content-Type: application/json
// Content-Length: 29
// Connection: close
//
// {"name":"foo","enabled":true}

pub async fn http1_handle(preface_message: &str,
                          mut reader: BufReader<OwnedReadHalf>,
                          mut writer: BufWriter<OwnedWriteHalf>) -> Result<()> {
    println!("{}", preface_message);
    writer.write("hello http1_handle".to_string().as_bytes()).await?;
    writer.flush().await?;

    if let Ok((method, path, version)) = parsing_preface(preface_message) {
        println!("method: {}, path: {}, version: {}", method, path, version)
    }

    // 만약 버퍼 사이즈 이상을 읽었는데도, Header를 다 못 읽었다면?
    let mut acc = Vec::with_capacity(1024);
    loop {
        // read_until(...).await가 Ok(0)을 돌려주는 건 “느리다”가 아니라 진짜 EOF(연결이 닫힘)임.
        let read_size = reader.read_until(b'\n', &mut acc).await?;
        if read_size == 0 {
            bail!("EOF before header termination.")
        }

        // read_until(b'\n') ensure this.
        if acc.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let header_string = String::from_utf8(acc).expect("Invalid UTF-8 sequence");

    println!("Header String \n
{}", header_string);


    Ok(())
}


fn parsing_preface(preface_message: &str) -> Result<(String, String, String)> {

    let mut it = preface_message.split_whitespace();
    let (method, path, version) = match (it.next(), it.next(), it.next()) {
        (Some(m), Some(p), Some(v)) => (m, p, v),
        _ =>  return bail!("need at least 3 tokens")
    };

    Ok((method.to_string(), path.to_string(), version.to_string()))
}




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