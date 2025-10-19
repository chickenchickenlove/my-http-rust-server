use std::collections::HashMap;
use bytes::{
    Bytes
};
use fluke_h2_parse::{
    ContinuationFlags,
    Frame,
    FrameType
};
use crate::http2::common_frame_facade::{
    h2_frame_to_bytes,
    FrameFacade
};
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http_connection_context::Http2ConnectionContext1;

#[derive(Debug)]
pub struct ContinuationFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.10
//    The CONTINUATION frame (type=0x9) is used to continue a sequence of
//    header block fragments (Section 4.3).
impl ContinuationFrameFacade {

    //       +---------------------------------------------------------------+
    //     |                   Header Block Fragment (*)                 ...
    //     +---------------------------------------------------------------+
    //
    //                    Figure 15: CONTINUATION Frame Payload
    //
    pub fn with(frame: Frame, payload: Bytes) -> Self {
        ContinuationFrameFacade { frame, payload }
    }
    pub fn decode_payload(&self) -> anyhow::Result<HashMap<String, String>> {
        let mut decoder = fluke_hpack::Decoder::new();
        decoder.set_max_table_size(4096);

        let mut headers = HashMap::new();
        if let Ok(decoded_header_pairs) = decoder.decode(&self.payload) {
            for (kk, vv) in decoded_header_pairs.into_iter() {
                let kkk = String::from_utf8(kk)?;
                let vvv = String::from_utf8(vv)?;
                headers.insert(kkk, vvv);
            }
        }

        Ok(headers)
    }

    pub fn get_header_fragment(&mut self) -> anyhow::Result<Bytes> {
        Ok(self.payload.clone())
    }

    pub fn is_end_headers(&self) -> bool {
        let flags = match self.frame.frame_type {
            FrameType::Continuation(f) => Some(f),
            _ => None,
        };

        if let Some(headers_flags) = flags {
            return headers_flags.contains(ContinuationFlags::EndHeaders);
        }
        false
    }
}




pub struct ContinuationFrameHandler { }

impl ContinuationFrameHandler {
    pub fn new() -> Self {
        ContinuationFrameHandler {}
    }
}


impl FrameHandler for ContinuationFrameHandler {

    //
    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        // #1
        //  CONTINUATION frames MUST be associated with a stream.  If a
        //    CONTINUATION frame is received whose stream identifier field is 0x0,
        //    the recipient MUST respond with a connection error (Section 5.4.1) of
        //    type PROTOCOL_ERROR.
        if frame.stream_id() == 0 {
            return Err(
                Http2Error::connection_error(ErrorCode::ProtocolError).into()
            );
        }

        Ok(())
    }


    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }

    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        Ok(None)
    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {

        let sid = frame.stream_id();
        if !frame.has_end_headers_flag() && conn_ctx.equal_inflight_headers_stream_id(sid) {
            conn_ctx.update_inflight_headers(frame.stream_id())
        }

        if frame.has_end_headers_flag() && conn_ctx.equal_inflight_headers_stream_id(sid) {
            conn_ctx.clear_inflight_headers()
        }


        Ok(())
    }
}

impl TryInto<Bytes> for ContinuationFrameFacade {

    type Error = anyhow::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        h2_frame_to_bytes(self.frame, &self.payload)
    }
}
