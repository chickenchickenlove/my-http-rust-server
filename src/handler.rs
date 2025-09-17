use std::collections::HashMap;
use anyhow::{Result, Error, bail};
use tokio::io::{split, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};


struct Http1ConnectionContext {
    req_context: Http1RequestContext,
}

// 러스트에서는 모든 필드를 한번에 초기화해야한다.
// 따라서 아직, 모르는 값을 나중에 채우는 방식은 안되기 때문에 Builder 패턴을 쓴다.
// #[derive!(Debug)]
struct Http1ConnectionContextBuilder {
    req_context: Option<Http1ConnectionContext>
}
// #[derive!(Debug)]
impl Http1ConnectionContextBuilder {
    fn new() -> Http1ConnectionContextBuilder {
        Http1ConnectionContextBuilder { req_context: None }
    }
}

// #[derive!(Debug)]
struct Http1RequestContext {
    method: String,
    path: String,
    version: String,
    // protocol: String,
    headers: HashMap<String, String>,
    body: Option<String>
}

// #[derive!(Debug)]
struct Http1RequestContextBuilder {
    method: Option<String>,
    path: Option<String>,
    version: Option<String>,
    // protocol: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<String>
}

// #[derive!(Debug)]
impl Http1RequestContextBuilder {
    fn new() -> Self {
        Http1RequestContextBuilder { method: None, path: None, version: None, headers: None, body: None }
    }

    fn build(self) -> Http1RequestContext {
        Http1RequestContext {
            method: self.method.unwrap(),
            path: self.path.unwrap(),
            version: self.version.unwrap(),
            // protocol: self.protocol.unwrap(),
            headers: self.headers.unwrap(),
            body: self.body,
        }
    }

    fn method(&mut self, method: String) -> &mut Self {
        self.method.replace(method);
        self
    }

    fn path(&mut self, path: String) -> &mut Self {
        self.path.replace(path);
        self
    }

    fn version(&mut self, version: String) -> &mut Self {
        self.version.replace(version);
        self
    }

    fn headers(&mut self, headers: HashMap<String, String>) -> &mut Self {
        self.headers.replace(headers);
        self
    }

    fn body(&mut self, body: String) -> &mut Self {
        self.body.replace(body);
        self
    }

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


    let mut conn_ctx_builder = Http1ConnectionContextBuilder::new();
    let mut req_ctx_builder = Http1RequestContextBuilder::new();

    if let Ok((method, path, version)) = parse_preface(preface_message) {
        println!("method: {}, path: {}, version: {}", method, path, version);
        req_ctx_builder.method(method).path(path).version(version);
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
    let headers = parse_header(header_string);
    println!("Header String \n{:?}", headers);

    let maybe_body_length = headers.get("content-length");
    let body = parse_body(maybe_body_length, &mut reader).await;

    req_ctx_builder.headers(headers);

    if let Some(body_string) = body {
        println!("Body: {}", body_string);
        req_ctx_builder.body(body_string);
    }
    // TODO - "Transfer-Encoding : Chunk"인 경우, Content-Length보다 우선순위를 높여서 Body를 읽어야 한다.

    let mut req_ctx = req_ctx_builder.build();
    // Http1ConnectionContext{method: }


    Ok(())
}


fn parse_preface(preface_message: &str) -> Result<(String, String, String)> {

    let mut it = preface_message.split_whitespace();
    let (method, path, version) = match (it.next(), it.next(), it.next()) {
        (Some(m), Some(p), Some(v)) => (m, p, v),
        _ =>  return bail!("need at least 3 tokens")
    };

    Ok((method.to_string(), path.to_string(), version.to_string()))
}

fn parse_header(header_string: String) -> HashMap<String, String> {
    let mut headers = HashMap::new();

    for line in header_string.split("\r\n") {
        if line.is_empty() {
            break;
        }
        let mut kv = line.split(": ");
        match (kv.next(), kv.next()) {
            (Some(key), Some(value)) => headers.insert(key.to_ascii_lowercase().to_string(), value.to_string()),
            _ => continue
        };
    }

    headers
}

async fn parse_body(content_length: Option<&String>, reader: &mut BufReader<OwnedReadHalf>) -> Option<String> {
    match content_length {
        None => None,
        Some(value) => {
            if let Ok(content_length) = value.parse::<usize>() {
                // let mut buf = Vec::with_capacity(content_length as usize); // <- 이건 잘 동작하지 않음. Capacity와 Length 차이 확인.
                let mut buf = vec![0u8; content_length];
                reader.read_exact(&mut buf).await.unwrap();
                if let Ok(body) = String::from_utf8(buf.to_vec()) {
                    Some(body)
                }
                else {
                    None
                }
            } else {
                None
            }
        }
    }
}