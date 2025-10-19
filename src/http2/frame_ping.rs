use anyhow::{anyhow, bail};
use bytes::Bytes;
use fluke_h2_parse::{Frame, FrameType, PingFlags, StreamId};
use fluke_h2_parse::enumflags2::BitFlags;
use crate::http2::common_frame_facade::h2_frame_to_bytes;
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::common_frame_facade::{FrameFacade};
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http_connection_context::Http2ConnectionContext1;

#[derive(Debug)]
pub struct PingFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.7
//    The PING frame (type=0x6) is a mechanism for measuring a minimal
//    round-trip time from the sender, as well as determining whether an
//    idle connection is still functional.
impl PingFrameFacade {

    //     +---------------------------------------------------------------+
    //     |                                                               |
    //     |                      Opaque Data (64)                         |
    //     |                                                               |
    //     +---------------------------------------------------------------+
    //
    //                       Figure 12: PING Payload Format
    //    In addition to the frame header, PING frames MUST contain 8 octets of
    //    opaque data in the payload.  A sender can include any value it
    //    chooses and use those octets in any fashion.
    //    8 octets = 8 bytes = 64 bits.
    //    1 octet = 1 byte = 8 bits

    pub fn with(frame: Frame, payload: Bytes) -> Self {
        PingFrameFacade { frame, payload }
    }

    pub fn decode_payload(mut payload: Bytes) -> anyhow::Result<Bytes> {
        // id :  2 Byte
        // value : 4 Byte
        Ok(payload)
    }
}


// https://datatracker.ietf.org/doc/html/rfc7540#section-6.7
pub struct PingFrameHandler { }

impl PingFrameHandler {
    pub fn new() -> Self {
        PingFrameHandler {}
    }
}


impl FrameHandler for PingFrameHandler {

    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }
    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        // # 1
        //    PING frames are not associated with any individual stream.  If a PING
        //    frame is received with a stream identifier field value other than
        //    0x0, the recipient MUST respond with a connection error
        //    (Section 5.4.1) of type PROTOCOL_ERROR.
        if frame.stream_id() != 0 {
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
        }

        //    Receipt of a PING frame with a length field value other than 8 MUST
        //    be treated as a connection error (Section 5.4.1) of type
        //    FRAME_SIZE_ERROR.
        if frame.get_payload_size() != 8 {
            return Err(Http2Error::connection_error(ErrorCode::FrameSizeError).into());
        }

        Ok(())
    }

    //    Receivers of a PING frame that does not include an ACK flag MUST send
    //    a PING frame with the ACK flag set in response, with an identical
    //    payload.  PING responses SHOULD be given higher priority than any
    //    other frame.
    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        let is_ack = match frame.frame_type() {
            FrameType::Ping(flags) => flags.contains(PingFlags::Ack),
            _ => bail!("Unexpected state")
        };

        if !is_ack {
            let ack_frame = Frame::new(
                FrameType::Ping(BitFlags::from(PingFlags::Ack)),
                StreamId(0),
            );

            Ok(
                Some(FrameFacade::PingFrame(PingFrameFacade::with(ack_frame, frame.payload())))
            )
        } else {
            Ok(None)
        }

    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }
}

impl TryInto<Bytes> for PingFrameFacade {

    type Error = anyhow::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        h2_frame_to_bytes(self.frame, &self.payload)
    }
}