use std::collections::HashMap;
use std::ops::BitAnd;
use anyhow::anyhow;
use bytes::{Buf, Bytes};
use fluke_h2_parse::{ContinuationFlags, DataFlags, Frame, FrameType, HeadersFlags};
use fluke_h2_parse::enumflags2::BitFlag;
use crate::http2::common_frame_facade::{h2_frame_to_bytes, FrameFacade};
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::frame_ping::PingFrameFacade;
use crate::http_connection_context::Http2ConnectionContext1;

#[derive(Debug)]
pub struct UnknownFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.10
//    The CONTINUATION frame (type=0x9) is used to continue a sequence of
//    header block fragments (Section 4.3).
impl UnknownFrameFacade {

    pub fn with(frame: Frame, payload: Bytes) -> Self {
        UnknownFrameFacade { frame, payload }
    }
}