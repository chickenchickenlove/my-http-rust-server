use std::collections::HashMap;
use std::ops::BitAnd;
use anyhow::anyhow;
use bytes::{BufMut, Bytes, BytesMut};
use fluke_h2_parse::{Frame, FrameType, HeadersFlags, StreamId};
use crate::http2::common_frame_facade::{h2_frame_to_bytes, FrameFacade};
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::frame_ping::PingFrameFacade;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http_connection_context::Http2ConnectionContext1;

#[derive(Debug)]
pub struct RstStreamFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.4
impl RstStreamFrameFacade {

    //    The RST_STREAM frame (type=0x3) allows for immediate termination of a
    //    stream.  RST_STREAM is sent to request cancellation of a stream or to
    //    indicate that an error condition has occurred.

    //     +---------------------------------------------------------------+
    //     |                        Error Code (32)                        |
    //     +---------------------------------------------------------------+
    //
    //                     Figure 9: RST_STREAM Frame Payload

    pub fn with(frame: Frame, payload: Bytes) -> Self {
        RstStreamFrameFacade { frame, payload }
    }

    pub fn with_error(stream_id: u32, error_code: ErrorCode) -> Self {
        let mut payload = BytesMut::with_capacity(4);
        let code: u32 = error_code.into();
        payload.put_u32(code);

        let frame = Frame::new(FrameType::RstStream, StreamId(stream_id));
        Self::with(frame, payload.freeze())
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

}




pub struct RstStreamFrameHandler { }

impl RstStreamFrameHandler {
    pub fn new() -> Self {
        RstStreamFrameHandler {}
    }
}


impl FrameHandler for RstStreamFrameHandler {

    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }

    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        // # 1
        //    RST_STREAM frames MUST be associated with a stream.  If a RST_STREAM
        //    frame is received with a stream identifier of 0x0, the recipient MUST
        //    treat this as a connection error (Section 5.4.1) of type
        //    PROTOCOL_ERROR.
        if frame.stream_id() == 0 {
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
        }

        //    A RST_STREAM frame with a length other than 4 octets MUST be treated
        //    as a connection error (Section 5.4.1) of type FRAME_SIZE_ERROR.
        if frame.get_payload_size() != 4 {
            return Err(Http2Error::connection_error(ErrorCode::FrameSizeError).into());
        }

        Ok(())
    }

    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        Ok(None)
    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }
}

impl TryInto<Bytes> for RstStreamFrameFacade {

    type Error = anyhow::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        h2_frame_to_bytes(self.frame, &self.payload)
    }
}