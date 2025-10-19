use bytes::{
    Buf,
    BufMut,
    Bytes,
    BytesMut
};
use fluke_h2_parse::{
    DataFlags,
    Frame,
    FrameType,
    HeadersFlags,
    StreamId
};
use crate::http2::common_frame_facade::{
    h2_frame_to_bytes,
    FrameFacade
};
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::http2_errors::{
    ErrorCode,
    Http2Error
};
use crate::http_connection_context::Http2ConnectionContext1;

#[derive(Debug)]
pub struct GoawayFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.8
//    The GOAWAY frame (type=0x7) is used to initiate shutdown of a
//    connection or to signal serious error conditions.  GOAWAY allows an
//    endpoint to gracefully stop accepting new streams while still
//    finishing processing of previously established streams.  This enables
//    administrative actions, like server maintenance.
// Goaway Frame은 Connection 레벨에만 적용. (   The GOAWAY frame applies to the connection, not a specific stream.)
impl GoawayFrameFacade {

    //    +-+-------------------------------------------------------------+
    //     |R|                  Last-Stream-ID (31)                        |
    //     +-+-------------------------------------------------------------+
    //     |                      Error Code (32)                          |
    //     +---------------------------------------------------------------+
    //     |                  Additional Debug Data (*)                    |
    //     +---------------------------------------------------------------+
    //
    //                      Figure 13: GOAWAY Payload Format
    //
    pub fn with(frame: Frame, payload: Bytes) -> Self {
        GoawayFrameFacade { frame, payload }
    }

    pub fn with_error(last_seen_stream_id: u32, error_code: ErrorCode) -> Self
    {
        let goaway_frame = Frame::new(FrameType::GoAway, StreamId(0));

        let error: u32 = error_code.into();
        let payload_last_stream = 0x7FFF_FFF & last_seen_stream_id;
        let payload_error_code = error;

        let mut payload = BytesMut::with_capacity(8); // 4 + 4
        payload.put_u32(payload_last_stream);
        payload.put_u32(payload_error_code);

        Self::with(goaway_frame, payload.freeze())
    }

    // pub fn with_error_stream_id(stream_id: u32, error_code: ErrorCode) -> Self {
    //     Frame::new(FrameType::GoAway, )
    //
    //
    // }

    fn decode_payload_static(mut payload: Bytes) -> anyhow::Result<(u32, u32)> {
        let last_stream_id = payload.get_u32() & 0x7FFF_FFF;
        let error_code = payload.get_u32();
        Ok((last_stream_id, error_code))
    }

    pub fn get_last_stream_id(mut payload: Bytes) -> anyhow::Result<u32> {
        let (last_stream_id, _) = Self::decode_payload_static(payload)?;
        Ok(last_stream_id)
    }


    pub fn is_end_headers(&self) -> bool {
        let flags = match self.frame.frame_type {
            FrameType::Headers(f) => Some(f),
            _ => None,
        };

        if let Some(headers_flags) = flags {
            return headers_flags.contains(HeadersFlags::EndHeaders);
        }
        false
    }

    pub fn is_received_end_stream(&self) -> bool {
        let flags = match self.frame.frame_type {
            FrameType::Headers(f) => Some(f),
            _ => None,
        };

        if let Some(headers_flags) = flags {
            return headers_flags.contains(HeadersFlags::EndStream);
        }
        false
    }
    pub fn has_end_stream_flag(&self) -> bool {
        match self.frame.frame_type {
            FrameType::Headers(f) => {
                f.contains(HeadersFlags::EndStream)
            }
            FrameType::Data(f) => {
                f.contains(DataFlags::EndStream)
            }
            _ => false
        }
    }

}




pub struct GoawayFrameHandler { }

impl GoawayFrameHandler {
    pub fn new() -> Self {
        GoawayFrameHandler {}
    }
}


impl FrameHandler for GoawayFrameHandler {

    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }
    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        // #1
        //    The GOAWAY frame applies to the connection, not a specific stream.
        //    An endpoint MUST treat a GOAWAY frame with a stream identifier other
        //    than 0x0 as a connection error (Section 5.4.1) of type
        //    PROTOCOL_ERROR.

        if frame.stream_id() != 0 {
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
        }

        Ok(())
    }

    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        Ok(None)
    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        let last_stream_id = GoawayFrameFacade::get_last_stream_id(frame.payload())?;
        conn_ctx.set_go_away_stream_id(last_stream_id);
        Ok(())
    }
}

impl TryInto<Bytes> for GoawayFrameFacade {

    type Error = anyhow::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        h2_frame_to_bytes(self.frame, &self.payload)
    }
}