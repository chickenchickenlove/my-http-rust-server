use anyhow::{
    anyhow,
};
use bytes::{
    BytesMut,
    Bytes,
    BufMut,
};
use fluke_h2_parse::{ContinuationFlags, DataFlags, Frame, FrameType, HeadersFlags, StreamId};
use fluke_h2_parse::FrameType::{
    Continuation,
    Data,
    GoAway,
    Headers,
    Ping,
    Priority,
    PushPromise,
    RstStream,
    Settings,
    Unknown,
    WindowUpdate
};
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

#[derive(Debug)]
pub enum FrameFacade {
    SettingsFrame(SettingsFrameFacade),
    PingFrame(PingFrameFacade),
    PriorityFrame(PriorityFrameFacade),

    WindowUpdateFrame(WindowUpdateFrameFacade),
    HeadersFrame(HeadersFrameFacade),

    DataFrame(DataFrameFacade),
    RstStreamFrame(RstStreamFrameFacade),

    GoawayFrame(GoawayFrameFacade),
    ContinuationFrame(ContinuationFrameFacade),
    UnknownFrame(UnknownFrameFacade),
}

impl FrameFacade {

    pub fn frame_type(&self) -> FrameType {
        match self {
            FrameFacade::SettingsFrame(f) => f.frame.frame_type,
            FrameFacade::PingFrame(f) => f.frame.frame_type,
            FrameFacade::PriorityFrame(f) => f.frame.frame_type,
            FrameFacade::WindowUpdateFrame(f) => f.frame.frame_type,
            FrameFacade::HeadersFrame(f) => f.frame.frame_type,
            FrameFacade::DataFrame(f) => f.frame.frame_type,
            FrameFacade::RstStreamFrame(f) => f.frame.frame_type,
            FrameFacade::GoawayFrame(f) => f.frame.frame_type,
            FrameFacade::ContinuationFrame(f) => f.frame.frame_type,
            FrameFacade::UnknownFrame(f) => f.frame.frame_type,
        }
    }

    pub fn frame(&self) -> &Frame {
        match self {
            FrameFacade::SettingsFrame(f) => &f.frame,
            FrameFacade::PingFrame(f) => &f.frame,
            FrameFacade::PriorityFrame(f) => &f.frame,
            FrameFacade::WindowUpdateFrame(f) => &f.frame,
            FrameFacade::HeadersFrame(f) => &f.frame,
            FrameFacade::DataFrame(f) => &f.frame,
            FrameFacade::RstStreamFrame(f) => &f.frame,
            FrameFacade::GoawayFrame(f) => &f.frame,
            FrameFacade::ContinuationFrame(f) => &f.frame,
            FrameFacade::UnknownFrame(f) => &f.frame,
        }
    }

    pub fn stream_id(&self) -> u32 {
        match self {
            FrameFacade::SettingsFrame(f) => f.frame.stream_id.0,
            FrameFacade::PingFrame(f) => f.frame.stream_id.0,
            FrameFacade::PriorityFrame(f) => f.frame.stream_id.0,
            FrameFacade::WindowUpdateFrame(f) => f.frame.stream_id.0,
            FrameFacade::HeadersFrame(f) => f.frame.stream_id.0,
            FrameFacade::DataFrame(f) => f.frame.stream_id.0,
            FrameFacade::RstStreamFrame(f) => f.frame.stream_id.0,
            FrameFacade::GoawayFrame(f) => f.frame.stream_id.0,
            FrameFacade::ContinuationFrame(f) => f.frame.stream_id.0,
            FrameFacade::UnknownFrame(f) => f.frame.stream_id.0,
        }
    }

    pub fn payload(&self) -> Bytes {
        match self {
            FrameFacade::SettingsFrame(f) => f.payload.clone(),
            FrameFacade::PingFrame(f) => f.payload.clone(),
            FrameFacade::PriorityFrame(f) => f.payload.clone(),
            FrameFacade::WindowUpdateFrame(f) => f.payload.clone(),
            FrameFacade::HeadersFrame(f) => f.payload.clone(),
            FrameFacade::DataFrame(f) => f.payload.clone(),
            FrameFacade::RstStreamFrame(f) => f.payload.clone(),
            FrameFacade::GoawayFrame(f) => f.payload.clone(),
            FrameFacade::ContinuationFrame(f) => f.payload.clone(),
            FrameFacade::UnknownFrame(f) => f.payload.clone(),
        }
    }

    pub fn window_size(&mut self) -> u32 {
        match self {
            FrameFacade::SettingsFrame(f) => f.window_size().unwrap(),
            _ => 65535
        }
    }

    // FrameFacade::is_data_frame()은 match 안 쓰고 matches!로:
    //
    // pub fn is_data_frame(&self) -> bool {
    //     matches!(self, FrameFacade::DataFrame(_))
    // }
    pub fn is_data_frame(&self) -> bool {
        match self {
            FrameFacade::DataFrame(f) => true,
            _ => false
        }
    }

    pub fn get_payload_size(&self) -> usize {
        match self {
            FrameFacade::SettingsFrame(f) => f.payload.len(),
            FrameFacade::PingFrame(f) => f.payload.len(),
            FrameFacade::PriorityFrame(f) => f.payload.len(),
            FrameFacade::WindowUpdateFrame(f) => f.payload.len(),
            FrameFacade::HeadersFrame(f) => f.payload.len(),
            FrameFacade::DataFrame(f) => f.payload.len(),
            FrameFacade::RstStreamFrame(f) => f.payload.len(),
            FrameFacade::GoawayFrame(f) => f.payload.len(),
            FrameFacade::ContinuationFrame(f) => f.payload.len(),
            FrameFacade::UnknownFrame(f) => f.payload.len(),
        }
    }

    pub fn has_end_stream_flag(&self) -> bool {
        let frame_type = self.frame_type();
        match frame_type {
            FrameType::Headers(f) => {
                f.contains(HeadersFlags::EndStream)
            },
            FrameType::Data(f) => {
                f.contains(DataFlags::EndStream)
            },
            _ => false
        }
    }

    pub fn has_end_headers_flag(&self) -> bool {
        let frame_type = self.frame_type();
        match frame_type {
            FrameType::Headers(f) => {
                f.contains(HeadersFlags::EndHeaders)
            },
            FrameType::Continuation(f) => {
                f.contains(ContinuationFlags::EndHeaders)
            },
            _ => false
        }
    }

}

// 공용 헬퍼: 9바이트 헤더 + payload 쓰기
pub fn h2_frame_to_bytes(
    frame: Frame,
    payload: &[u8],   // ACK는 빈 페이로드
) -> anyhow::Result<Bytes> {


    if payload.len() > 0x00FF_FFFF {
        return Err(anyhow!("payload too large for 24-bit length"));
    }

    let frame_type_byte = match frame.frame_type {
        Data(_) => 0x00,
        Headers(_) => 0x01,
        Priority => 0x02,
        RstStream => 0x03,
        Settings(_) => 0x04,
        PushPromise => 0x05,
        Ping(_) => 0x06,
        GoAway => 0x07,
        WindowUpdate => 0x08,
        Continuation(_) => 0x09,
        Unknown(_) => 0x0a,
        _ => 0x0a
    };

    let flags = match frame.frame_type {
        Data(flags) => flags.bits(),
        Headers(flags) => flags.bits(),
        Settings(flags) => flags.bits(),
        Ping(flags) => flags.bits(),
        Continuation(flags) => flags.bits(),
        _ => 0,
    };

    let sid  = frame.stream_id.0 & 0x7FFF_FFFF;
    let mut out = BytesMut::with_capacity(9 + payload.len());
    let len = payload.len() as u32;

    // length (24-bit, big-endian)
    out.put_u8(((len >> 16) & 0xFF) as u8);
    out.put_u8(((len >>  8) & 0xFF) as u8);
    out.put_u8(( len        & 0xFF) as u8);

    // type, flags
    out.put_u8(frame_type_byte);
    out.put_u8(flags);

    // stream id (31-bit)
    out.put_u8(((sid >> 24) & 0xFF) as u8);
    out.put_u8(((sid >> 16) & 0xFF) as u8);
    out.put_u8(((sid >>  8) & 0xFF) as u8);
    out.put_u8(( sid        & 0xFF) as u8);

    // Attach payload.
    out.extend_from_slice(payload);

    Ok(out.freeze())
}