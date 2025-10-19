use std::collections::HashMap;
use std::io::BufRead;
use std::ptr::read;
use std::time::Duration;
use anyhow::{
    Result,
    bail,
    Context,
    anyhow};
use bytes::{Buf, Bytes, BytesMut};
use fluke_buffet::bufpool::{
    BufMut
};
use tokio::io::{
    AsyncBufReadExt,
    AsyncRead,
    AsyncReadExt,
    BufReader};
use tokio::net::tcp::{
    OwnedReadHalf
};
use crate::http_connection_context::{
    Http1ConnectionContext1,
    Http11ConnectionContext1,
    Http2ConnectionContext1
};
use crate::http_request_context::{
    RequestContext,
    Http1RequestContext,
    Http11RequestContext,
    RequestContextBuilder
};

use fluke_buffet::Roll;
use fluke_h2_parse::{preface, Frame, FrameType};
use tokio::time::{sleep, timeout};
use crate::connection::PrefaceMessage;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::ContinuationFrame;
use crate::http2::common_frame_handler::FrameHandle;
use crate::http2::frame_continuation::ContinuationFrameFacade;
use crate::http2::frame_data::DataFrameFacade;
use crate::http2::frame_goaway::GoawayFrameFacade;
use crate::http2::frame_headers::HeadersFrameFacade;
use crate::http2::frame_ping::PingFrameFacade;
use crate::http2::frame_priority::PriorityFrameFacade;
use crate::http2::frame_rst_stream::RstStreamFrameFacade;
use crate::http2::frame_settings::SettingsFrameFacade;
use crate::http2::frame_unknown::UnknownFrameFacade;
use crate::http2::frame_window_update::WindowUpdateFrameFacade;
use crate::http_status::HttpStatus::OK;

pub trait GeneralHttp1Handler1<R>
where
    R: RequestContext + TryFrom<RequestContextBuilder<R>>,
{

    async fn handle_(&mut self,
                     preface_message: Option<PrefaceMessage>,
                     reader: &mut BufReader<OwnedReadHalf>) -> Result<R>
    {
        let mut req_ctx_builder = RequestContextBuilder::<R>::new();
        let preface_msg = match preface_message {
            // If First request
            Some(v) => v,
            // After First
            None => {
                let mut read_buf = Vec::with_capacity(1024);
                reader.read_until(b'\n', &mut read_buf).await?;
                PrefaceMessage::with(read_buf)
            }
        };

        let preface = String::from_utf8(preface_msg.0.to_vec())?;
        if let Ok((method, path, version)) = self.parse_preface(preface.as_str()) {
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

        let header_string = String::from_utf8(acc)
            .context("Invalid UTF-8 sequence")?;

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

        Ok(req_ctx_builder.build()?)
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

impl GeneralHttp1Handler1<Http1RequestContext> for Http1Handler { }


pub struct Http11Handler;

impl GeneralHttp1Handler1<Http11RequestContext> for Http11Handler { }
impl Http11Handler {
    pub fn new() -> Self {
        Http11Handler {}
    }
}


pub struct Http2Handler {}

impl Http2Handler {

    pub fn new() -> Self {
        Http2Handler {}
    }

    pub async fn handle_preface(&mut self,
                                 pre_buf: &[u8],
                                 reader: &mut BufReader<OwnedReadHalf>) -> Result<()>
    {
        let mut bytes_mut = BytesMut::from(pre_buf);
        let mut rest = [0u8; 8];
        reader.read_exact(&mut rest).await?;

        bytes_mut.extend_from_slice(&rest);
        let mut bm = BufMut::alloc().expect("buf alloc");
        let (mut preface_buf, _) = bm.split_at(bytes_mut.len());
        preface_buf.copy_from_slice(
            &bytes_mut[..bytes_mut.len()]
        );

        let roll: Roll = preface_buf
            .freeze()
            .into();

        match preface(roll) {
            Ok((_rest, _magic)) => (),
            Err(e) => { println!("HERE :{:?}", e) }
        }

        Ok(())
    }

    pub async fn handle_settings<'a>(&mut self,
                                 conn_ctx: &mut Http2ConnectionContext1,
                                 reader: &mut BufReader<OwnedReadHalf>
    ) -> Result<(FrameFacade, FrameFacade)> {
        // 9 means Header Frame bytes.
        let frame = {
            let mut read_buf = [0u8; 9];
            reader.read_exact(&mut read_buf).await?;

            let (mut buf, _) = BufMut::alloc()
                .expect("buf alloc")
                .split_at(read_buf.len());

            buf.copy_from_slice(&read_buf);

            let roll: Roll = buf
                .freeze()
                .into();

            match Frame::parse(roll) {
                Ok((_rest, frame)) => {
                    frame
                }
                Err(e) => {
                    println!("Failed to parsing frame from roll. {:?}", e);
                    bail!("Error");
                }
            }
        };

        // Capacity - 용량을 의미함. 실제 사이즈는 0임.
        // 사이즈를 늘리려면 resize를 사용해야함.
        let mut payload_buf = BytesMut::with_capacity(frame.len as usize);
        payload_buf.resize(frame.len as usize, 0);
        reader.read_exact(&mut payload_buf).await?;


        let settings_frame = SettingsFrameFacade::with(frame, payload_buf.freeze());
        let mut frame = FrameFacade::SettingsFrame(settings_frame);

        let maybe_ack_frame = FrameHandle::new()
            .handle(conn_ctx, &mut frame)
            .await?
            .unwrap();

        Ok((frame, maybe_ack_frame))
    }

    pub async fn parse_frame(&mut self,
                             reader: &mut BufReader<OwnedReadHalf>
    ) -> Result<Option<FrameFacade>>
    {
        println!("HERE: Try to parse frame. ");
        // 9 means Header Frame bytes.
        let frame = {
            let mut read_buf = [0u8; 9];

            // Socket이 닫혀있는 경우 Err가 발생한다.
            // 이 부분을 ?로 던지게 되면 Panic이 위로 올라가는 것처럼 나온다.
            if let Err(e) = reader.read_exact(&mut read_buf).await {
                bail!("Maybe, Connection already is closed. {:?}", e)
            }
            let (mut buf, _) = BufMut::alloc()
                .expect("buf alloc")
                .split_at(read_buf.len());

            buf.copy_from_slice(
                &read_buf[..]
            );

            let roll: Roll = buf
                .freeze()
                .into();

            match Frame::parse(roll) {
                Ok((_rest, frame)) => frame,
                Err(e) => bail!("Error"),
            }
        };

        let frame_facade = {
            let mut payload_buf = BytesMut::with_capacity(frame.len as usize);
            payload_buf.resize(frame.len as usize, 0);

            reader.read_exact(&mut payload_buf).await?;
            let payload = payload_buf.freeze();

            println!("ASH frame: {:?}", frame);

            match &frame.frame_type {
                FrameType::Settings(_) => Some(FrameFacade::SettingsFrame(
                    SettingsFrameFacade::with(frame, payload)
                )),
                FrameType::Ping(_) => Some(FrameFacade::PingFrame(
                    PingFrameFacade::with(frame, payload),
                )),
                FrameType::Priority => Some(FrameFacade::PriorityFrame(
                    PriorityFrameFacade::with(frame, payload)
                )),
                FrameType::WindowUpdate => Some(FrameFacade::WindowUpdateFrame(
                    WindowUpdateFrameFacade::with(frame, payload)
                )),
                FrameType::Continuation(_) => Some(ContinuationFrame(
                    ContinuationFrameFacade::with(frame, payload)
                )),
                FrameType::Headers(_) => Some(FrameFacade::HeadersFrame(
                    HeadersFrameFacade::with(frame, payload)
                )),
                FrameType::RstStream => Some(FrameFacade::RstStreamFrame(
                    RstStreamFrameFacade::with(frame, payload)
                )),
                FrameType::Data(_) => Some(FrameFacade::DataFrame(
                    DataFrameFacade::with(frame, payload)
                )),
                FrameType::GoAway => Some(FrameFacade::GoawayFrame(
                    GoawayFrameFacade::with(frame, payload)
                )),
                FrameType::Unknown(_) => Some(FrameFacade::UnknownFrame(
                    UnknownFrameFacade::with(frame, payload)
                )),
                _ => None,
            }
        };

        println!("Before OK frame_facade {:?}", frame_facade);
        Ok(frame_facade)
    }

    pub async fn handle_parsed_frame<'a>(&mut self,
                                     conn_ctx: &mut Http2ConnectionContext1,
                                     frame: &mut FrameFacade)
    -> Result<Option<FrameFacade>>
    {
        let maybe_ack_frame = FrameHandle::new()
            .handle(conn_ctx, frame)
            .await?;

        Ok(maybe_ack_frame)
    }

}

pub enum HttpConnectionContext1 {
    HTTP1Context(Http1ConnectionContext1),
    HTTP11Context(Http11ConnectionContext1),
    HTTP2Context(Http2ConnectionContext1),
}