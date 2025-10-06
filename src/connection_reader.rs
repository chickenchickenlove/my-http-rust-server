use std::collections::HashMap;
use std::io::BufRead;
use std::str::FromStr;
use anyhow::{Result, bail, Context, anyhow};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf};
use crate::connection_reader::HttpConnectionContext::{HTTP1Context, HTTP11Context};
use crate::http_connection_context::{
    ConnectionContextBuilder,
    Http1ConnectionContext,
    Http11ConnectionContext,
};
use crate::http_request_context::{Http1RequestContextBuilder, Http11RequestContextBuilder, Http1RequestContext, Http11RequestContext, RequestContextBuilder};

pub trait HttpConnectionReader {

    // 원래는 다음과 같이 가려고 했으나 동적 디스패치로 인한 hot path의 성능 열화가 걱정되었음.
    // 이 부분은 Enum 패턴 매칭을 이용할 수 있다.
    // async fn handle(&mut self, preface_message: &str) -> Result<(HttpConnectionContext)>;

    // Enum으로 정적 디스패치로 처리가능하다.
    async fn handle(&mut self,
                    preface_message: &str,
                    reader: &mut BufReader<OwnedReadHalf>) -> Result<HttpConnectionContext>;
}

pub struct Http1Handler;

impl Http1Handler {
    pub fn new() -> Self {
        Http1Handler {}
    }

    fn parse_header(&self, header_string: String) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        for line in header_string.split("\r\n") {
            if line.is_empty() {
                break;
            }
            // Don't use line.split(": ") because of invalid request
            let mut kv = line.splitn(2, ':');
            match (kv.next(), kv.next()) {
                (Some(key), Some(value)) =>
                    headers.insert(key.trim().to_ascii_lowercase().to_string(),
                                   value.trim().to_string()),
                _ => continue
            };
        }

        headers
    }

    async fn parse_body(&mut self,
                        reader: &mut BufReader<OwnedReadHalf>,
                        content_length: Option<&String>) -> Result<Option<Vec<u8>>> {
        match content_length {
            None => Ok(None),
            Some(value) => {
                let content_length = value.parse::<usize>()
                    .context("failed to type cast usize for content_length.")?;

                // let mut buf = Vec::with_capacity(content_length as usize); // <- 이건 잘 동작하지 않음. Capacity와 Length 차이 확인.
                let mut buf = vec![0u8; content_length];
                reader.read_exact(&mut buf).await?;
                Ok(Some(buf))
            }
        }
    }

    pub fn parse_preface(&self, preface_message: &str) -> Result<(String, String, String)> {
        let mut it = preface_message.split_whitespace();
        let (method, path, version) = match (it.next(), it.next(), it.next()) {
            (Some(m), Some(p), Some(v)) => (m, p, v),
            _ => return bail!("need at least 3 tokens")
        };

        Ok((method.to_string(), path.to_string(), version.to_string()))
    }
}

impl HttpConnectionReader for Http1Handler {
    async fn handle(&mut self, preface_message: &str, reader: &mut BufReader<OwnedReadHalf>) -> Result<HttpConnectionContext> {
        let mut conn_ctx_builder =
            ConnectionContextBuilder::<Http1RequestContext, Http1ConnectionContext>::new();
        // let mut conn_ctx_builder = Http1ConnectionContextBuilder::new();
        let mut req_ctx_builder = RequestContextBuilder::<Http1RequestContext>::new();
        // let mut req_ctx_builder = Http1RequestContextBuilder::new();

        if let Ok((method, path, version)) = self.parse_preface(preface_message) {
            println!("method: {}, path: {}, version: {}", method, path, version);
            req_ctx_builder
                .method(method.parse().map_err(|_| anyhow!("invalid HTTP method"))?)
                .path(path)
                .version(version);
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

        let header_string = String::from_utf8(acc).context("Invalid UTF-8 sequence")?;
        let headers = self.parse_header(header_string);
        println!("Header String \n{:?}", headers);

        let maybe_body_length = headers.get("content-length");
        let maybe_body = self.parse_body(reader, maybe_body_length)
            .await
            .context("Failed to parse body because it is invalid.")?;

        req_ctx_builder.headers(headers);

        if let Some(body_string) = maybe_body {
            req_ctx_builder.body(body_string);
        }
        // TODO - "Transfer-Encoding : Chunk"인 경우, Content-Length보다 우선순위를 높여서 Body를 읽어야 한다.

        let req_ctx = req_ctx_builder.build()?;

        conn_ctx_builder.req_ctx(req_ctx);
        Ok(HTTP1Context(conn_ctx_builder.build()?))
    }

}


pub struct Http11Handler;

impl Http11Handler {
    pub fn new() -> Self {
        Http11Handler {}
    }

    fn parse_header(&self, header_string: String) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        for line in header_string.split("\r\n") {
            if line.is_empty() {
                break;
            }
            // Don't use line.split(": ") because of invalid request
            let mut kv = line.splitn(2, ':');
            match (kv.next(), kv.next()) {
                (Some(key), Some(value)) =>
                    headers.insert(key.trim().to_ascii_lowercase().to_string(),
                                   value.trim().to_string()),
                _ => continue
            };
        }

        headers
    }

    async fn parse_body(&mut self,
                        reader: &mut BufReader<OwnedReadHalf>,
                        content_length: Option<&String>) -> Result<Option<Vec<u8>>> {
        match content_length {
            None => Ok(None),
            Some(value) => {
                let content_length = value.parse::<usize>()
                    .context("failed to type cast usize for content_length.")?;

                // let mut buf = Vec::with_capacity(content_length as usize); // <- 이건 잘 동작하지 않음. Capacity와 Length 차이 확인.
                let mut buf = vec![0u8; content_length];
                reader.read_exact(&mut buf).await?;
                Ok(Some(buf))
            }
        }
    }

    pub fn parse_preface(&self, preface_message: &str) -> Result<(String, String, String)> {
        let mut it = preface_message.split_whitespace();
        let (method, path, version) = match (it.next(), it.next(), it.next()) {
            (Some(m), Some(p), Some(v)) => (m, p, v),
            _ => return bail!("need at least 3 tokens")
        };

        Ok((method.to_string(), path.to_string(), version.to_string()))
    }
}

impl HttpConnectionReader for Http11Handler {
    async fn handle(&mut self, preface_message: &str, reader: &mut BufReader<OwnedReadHalf>) -> Result<HttpConnectionContext> {

        let mut conn_ctx_builder =
            ConnectionContextBuilder::<Http11RequestContext, Http11ConnectionContext>::new();
        // let mut conn_ctx_builder = Http11ConnectionContextBuilder::new();
        let mut req_ctx_builder = Http11RequestContextBuilder::new();

        if let Ok((method, path, version)) = self.parse_preface(preface_message) {
            println!("method: {}, path: {}, version: {}", method, path, version);
            req_ctx_builder
                .method(method.parse().map_err(|_| anyhow!("invalid HTTP method"))?)
                .path(path)
                .version(version);
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

        let header_string = String::from_utf8(acc).context("Invalid UTF-8 sequence")?;
        let headers = self.parse_header(header_string);
        println!("Header String \n{:?}", headers);

        let maybe_body_length = headers.get("content-length");
        let maybe_body = self.parse_body(reader, maybe_body_length)
            .await
            .context("Failed to parse body because it is invalid.")?;

        req_ctx_builder.headers(headers);

        if let Some(body_string) = maybe_body {
            req_ctx_builder.body(body_string);
        }
        // TODO - "Transfer-Encoding : Chunk"인 경우, Content-Length보다 우선순위를 높여서 Body를 읽어야 한다.

        let req_ctx = req_ctx_builder.build()?;

        conn_ctx_builder.req_ctx(req_ctx);
        Ok(HTTP11Context(conn_ctx_builder.build()?))
    }

}


pub enum HttpConnectionContext {
    HTTP1Context(Http1ConnectionContext),
    HTTP11Context(Http11ConnectionContext),
}

