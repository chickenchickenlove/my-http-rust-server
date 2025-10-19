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




// pub struct ContinuationFrameHandler { }
//
// impl ContinuationFrameHandler {
//     pub fn new() -> Self {
//         ContinuationFrameHandler {}
//     }
// }
//
//
// impl FrameHandler for ContinuationFrameHandler {
//
//     //
//     async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
//         // #1
//         //  CONTINUATION frames MUST be associated with a stream.  If a
//         //    CONTINUATION frame is received whose stream identifier field is 0x0,
//         //    the recipient MUST respond with a connection error (Section 5.4.1) of
//         //    type PROTOCOL_ERROR.
//
//         if frame.stream_id().0 == 0 {
//             anyhow!("ContinuationFrame don't allow stream is is 0.");
//         }
//
//         Ok(())
//     }
//
//     async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
//         Ok(None)
//     }
//
//     async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
//         Ok(())
//     }
// }
//
// impl TryInto<Bytes> for ContinuationFrameFacade {
//
//     type Error = anyhow::Error;
//
//     fn try_into(self) -> Result<Bytes, Self::Error> {
//         h2_frame_to_bytes(self.frame, &self.payload)
//     }
// }