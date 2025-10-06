use std::collections::HashMap;
use std::io::BufRead;
use std::ptr::read;
use anyhow::{Result, bail, Context, anyhow};
use bytes::{Buf, BytesMut};
use fluke_buffet::bufpool::{BufMut};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf};
use crate::connection_reader::HttpConnectionContext::{HTTP1Context, HTTP11Context};
use crate::http_connection_context::{ConnectionContextBuilder, Http1ConnectionContext, Http11ConnectionContext, ConnectionContext};
use crate::http_request_context::{
    RequestContext,
    Http1RequestContext,
    Http11RequestContext,
    RequestContextBuilder
};

use fluke_buffet::Roll;
use fluke_h2_parse::{
    preface,
    Frame,
};
use fluke_h2_parse::nom::IResult;
use crate::frame_handler::FrameHandle;

pub trait HttpConnectionReader {

    // 원래는 다음과 같이 가려고 했으나 동적 디스패치로 인한 hot path의 성능 열화가 걱정되었음.
    // 이 부분은 Enum 패턴 매칭을 이용할 수 있다.
    // async fn handle(&mut self, preface_message: &str) -> Result<(HttpConnectionContext)>;

    // Enum으로 정적 디스패치로 처리가능하다.
    async fn handle(&mut self,
                    preface_message: &str,
                    reader: &mut BufReader<OwnedReadHalf>) -> Result<HttpConnectionContext>;
}


pub trait GeneralHttp1Handler<R, C>
where
    R: RequestContext + TryFrom<RequestContextBuilder<R>>,
    C: ConnectionContext + From<R>,
    HttpConnectionContext: From<C>
{

    async fn handle_(&mut self,
                     preface_message: &str,
                     reader: &mut BufReader<OwnedReadHalf>) -> Result<HttpConnectionContext>
    {
        let mut conn_ctx_builder =
            ConnectionContextBuilder::<R, C>::new();
        let mut req_ctx_builder = RequestContextBuilder::<R>::new();

        if let Ok((method, path, version)) = self.parse_preface(preface_message) {
            println!("method: {}, path: {}, version: {}", method, path, version);
            req_ctx_builder
                .method(method.parse().map_err(|_| anyhow!("invalid HTTP method"))?)
                .path(path);
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

        // conn_ctx_builder.build()의 결과로 C가 반환된다.
        // 제네릭 타입 C의 타입으로 match를 할 수 없다.
        // 따라서, match가 아니라 변환 트레이트(From/Into)를 쓰는게 낫다.
        // 그러나 C를 타입별로 매칭할 수 없다.
        Ok(conn_ctx_builder.build()?.into())
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
                        content_length: Option<&String>) -> Result<Option<Vec<u8>>>
    {
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

    fn parse_preface(&self, preface_message: &str) -> Result<(String, String, String)> {
        let mut it = preface_message.split_whitespace();
        let (method, path, version) = match (it.next(), it.next(), it.next()) {
            (Some(m), Some(p), Some(v)) => (m, p, v),
            _ => return bail!("need at least 3 tokens")
        };

        Ok((method.to_string(), path.to_string(), version.to_string()))
    }
}

pub struct Http1Handler;


impl Http1Handler {
    pub fn new() -> Self {
        Http1Handler {}
    }

}

impl GeneralHttp1Handler<Http1RequestContext, Http1ConnectionContext> for Http1Handler {}

impl HttpConnectionReader for Http1Handler {
    async fn handle(&mut self, preface_message: &str, reader: &mut BufReader<OwnedReadHalf>) -> Result<HttpConnectionContext> {
        self.handle_(preface_message, reader).await
    }

}


pub struct Http11Handler;

impl GeneralHttp1Handler<Http11RequestContext, Http11ConnectionContext> for Http11Handler {}
impl Http11Handler {
    pub fn new() -> Self {
        Http11Handler {}
    }
}

impl HttpConnectionReader for Http11Handler {
    async fn handle(&mut self, preface_message: &str, reader: &mut BufReader<OwnedReadHalf>) -> Result<HttpConnectionContext> {
        self.handle_(preface_message, reader).await
    }

}


pub struct Http2Handler {}

impl Http2Handler {

    pub fn new() -> Self {
        Http2Handler {}
    }

    pub async fn hello(&self) {

    }
    pub async fn handle(&mut self,
                        pre_buf: &[u8],
                        reader: &mut BufReader<OwnedReadHalf>) -> Result<()> {
        let mut bytes_mut = BytesMut::with_capacity(16 * 1024);
        let mut preface_removed = false;
        let mut updated_frame = None;

        bytes_mut.extend_from_slice(pre_buf);

        loop {
            reader
                .read_buf(&mut bytes_mut)
                .await?;

            if !preface_removed {
                let mut bm = BufMut::alloc().expect("buf alloc");
                let (mut preface_buf, _) = bm.split_at(bytes_mut.len());

                preface_buf.copy_from_slice(
                    &bytes_mut[..bytes_mut.len()]
                );

                let roll: Roll = preface_buf
                    .freeze()
                    .into();

                match preface(roll) {
                    Ok((rest, _magic)) => {
                        println!("ASH hello rest : {:?}, magic: {:?}", rest, _magic);
                        bytes_mut.advance(bytes_mut.len() - rest.len());
                        preface_removed = true;
                    }
                    Err(e) => {
                        println!("HERE :{:?}", e)
                    }
                }
            }
            else {
                let mut bm = BufMut::alloc().expect("buf alloc");
                let (mut buf, _) = bm.split_at(bytes_mut.len());

                buf.copy_from_slice(
                    &bytes_mut[..bytes_mut.len()]
                );

                let roll: Roll = buf
                    .freeze()
                    .into();

                match Frame::parse(roll) {
                    Ok((rest, frame)) => {
                        println!("HERE :{:?}, frame: {:?}", rest, frame);
                        updated_frame.replace(frame);
                        bytes_mut.advance(bytes_mut.len() - rest.len());
                    }
                    Err(e) => {
                        println!("HERE :{:?}", e)
                    }
                }
            }

            if let Some(ref f) = updated_frame{
                FrameHandle::new().handle(f).await;
            }
        }
    }
}

pub enum HttpConnectionContext {
    HTTP1Context(Http1ConnectionContext),
    HTTP11Context(Http11ConnectionContext),
}



impl From<Http1ConnectionContext> for HttpConnectionContext {
    fn from(value: Http1ConnectionContext) -> Self {
        HTTP1Context(value)
    }
}

impl From<Http11ConnectionContext> for HttpConnectionContext {
    fn from(value: Http11ConnectionContext) -> Self {
        HTTP11Context(value)
    }
}

