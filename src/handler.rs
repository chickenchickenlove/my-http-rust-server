use std::collections::HashMap;
use std::io::BufRead;
use anyhow::{Result, Error, bail, anyhow};
use tokio::io::{split, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use crate::dispatcher::Method;
use crate::handler::HttpConnectionContext::HTTP1Context;

pub trait HttpConnectionHandler {

    // 원래는 다음과 같이 가려고 했으나 동적 디스패치로 인한 hot path의 성능 열화가 걱정되었음.
    // 이 부분은 Enum 패턴 매칭을 이용할 수 있다.
    // async fn handle(&mut self, preface_message: &str) -> Result<(HttpConnectionContext)>;

    // Enum으로 정적 디스패치로 처리가능하다.
    async fn handle(&mut self,
                    preface_message: &str,
                    reader: &mut BufReader<OwnedReadHalf>,
                    writer: &mut BufWriter<OwnedWriteHalf>) -> Result<(HttpConnectionContext)>;
}

pub struct Http1Handler;

impl Http1Handler {

    pub fn new() -> Self {
        Http1Handler{}
    }

    async fn parse_body(&mut self,
                        reader: &mut BufReader<OwnedReadHalf>,
                        writer: &mut BufWriter<OwnedWriteHalf>,
                        content_length: Option<&String>) -> Option<String> {
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

}

impl HttpConnectionHandler for Http1Handler {
    async fn handle(&mut self,
                    preface_message: &str,
                    reader: &mut BufReader<OwnedReadHalf>,
                    writer: &mut BufWriter<OwnedWriteHalf>) -> Result<HttpConnectionContext> {
        println!("{}", preface_message);

        let mut conn_ctx_builder = Http1ConnectionContextBuilder::new();
        let mut req_ctx_builder = Http1RequestContextBuilder::new();

        if let Ok((method, path, version)) = parse_preface(preface_message) {
            println!("method: {}, path: {}, version: {}", method, path, version);
            let method_enum = match method.as_str() {
                "GET"    => Method::GET,
                "POST"   => Method::POST,
                "DELETE" => Method::DELETE,
                "PUT"    => Method::PUT,
                _        => Method::UNSUPPORTED
            };
            req_ctx_builder.method(method_enum).path(path).version(version);
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
        let maybe_body = self.parse_body(reader, writer, maybe_body_length).await;

        req_ctx_builder.headers(headers);

        if let Some(body_string) = maybe_body {
            println!("Body: {}", body_string);
            req_ctx_builder.body(body_string);
        }
        // TODO - "Transfer-Encoding : Chunk"인 경우, Content-Length보다 우선순위를 높여서 Body를 읽어야 한다.

        let req_ctx = req_ctx_builder.build()?;

        conn_ctx_builder.req_ctx(req_ctx);
        Ok(HTTP1Context(conn_ctx_builder.build()))
    }
}


pub enum HttpConnectionContext {
    HTTP1Context(Http1ConnectionContext),
}


pub struct Http1ConnectionContext {
    req_context: Http1RequestContext,
}

impl Http1ConnectionContext {

    pub fn clone_req_ctx(&self) -> Http1RequestContext {
        self.req_context.clone()
    }

}


// 러스트에서는 모든 필드를 한번에 초기화해야한다.
// 따라서 아직, 모르는 값을 나중에 채우는 방식은 안되기 때문에 Builder 패턴을 쓴다.
// #[derive!(Debug)]
struct Http1ConnectionContextBuilder {
    req_context: Option<Http1RequestContext>
}
// #[derive!(Debug)]
impl Http1ConnectionContextBuilder {
    fn new() -> Http1ConnectionContextBuilder {
        Http1ConnectionContextBuilder { req_context: None }
    }

    fn req_ctx(&mut self, req_ctx: Http1RequestContext) -> &mut Self {
        self.req_context.replace(req_ctx);
        self
    }

    fn build(self) -> Http1ConnectionContext {
        Http1ConnectionContext { req_context: self.req_context.unwrap() }
    }
}

#[derive(Clone)]
pub struct Http1RequestContext {
    method: Method,
    path: String,
    version: String,
    // protocol: String,
    headers: HashMap<String, String>,
    body: Option<String>
}

impl Http1RequestContext {

    pub fn method(&self) -> Method {
        self.method
    }

    pub fn path(&self) -> String {
        self.path.clone()
    }

    pub fn version(&self) -> String {
        self.version.clone()
    }

    pub fn headers(&self) -> HashMap<String, String> {
        self.headers.clone()
    }

    pub fn body(&self) -> Option<String> {
        self.body.clone()
    }
}

// #[derive!(Debug)]
struct Http1RequestContextBuilder {
    method: Option<Method>,
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

    fn build(self) -> Result<Http1RequestContext> {
        // unwrap()에 의존. 파싱 실패 시 panic이 발생.
        // Result<Http1RequestContext>로 바꾸고 호출부에서 ? 처리.
        // match (self.method, self.path, self.version, self.headers) {
        //     (Some(m), Some(p), Some(v), Some(h)) =>
        //         Ok(Http1RequestContext{method: m, path: p, version: v, headers: h, body: self.body}),
        //     (_, _, _, _) => bail!("Invalid method or path or version or headers"),
        // }
        // 위 코드로 사용하면 1) 디버깅에 유연하지 않고, 2) 컴파일 에러가 발생할 수도 있음. (Self로 소유권 이동했다면)
        let method = self.method.ok_or_else(|| anyhow!("missing method."))?;
        let path = self.path.ok_or_else(|| anyhow!("missing path."))?;
        let version = self.version.ok_or_else(|| anyhow!("missing version."))?;
        let headers = self.headers.ok_or_else(|| anyhow!("missing headers."))?;

        Ok(
            Http1RequestContext {
                method,
                path,
                version,
                headers,
                body: self.body,
            }
        )
    }

    fn method(&mut self, method: Method) -> &mut Self {
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