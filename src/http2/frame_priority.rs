use anyhow::{anyhow, bail};
use bytes::{Buf, Bytes};
use fluke_h2_parse::Frame;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http_connection_context::Http2ConnectionContext1;

#[derive(Debug)]
pub struct PriorityFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,

    decoded_payload: Option<PriorityFramePayload>,

    // From Payload
    // exclusive: Option<bool>,
    // stream_dependency: Option<u32>,
    // weight: Option<u8>,
}


#[derive(Debug)]
pub struct PriorityFramePayload {
    exclusive: bool,
    stream_dependency: u32,
    weight: u8,
}

// RFC: https://datatracker.ietf.org/doc/html/rfc7540#section-6.3
impl PriorityFrameFacade {

    //     +-+-------------------------------------------------------------+
    //     |E|                  Stream Dependency (31)                     |
    //     +-+-------------+-----------------------------------------------+
    //     |   Weight (8)  |
    //     +-+-------------+
    //
    //                      Figure 8: PRIORITY Frame Payload
    //   E: A single-bit flag indicating that the stream dependency is
    //       exclusive (see Section 5.3).
    //
    //    Stream Dependency:  A 31-bit stream identifier for the stream that
    //       this stream depends on (see Section 5.3).
    //
    //    Weight:  An unsigned 8-bit integer representing a priority weight for
    //       the stream (see Section 5.3).  Add one to the value to obtain a
    //       weight between 1 and 256.
    pub fn with(frame: Frame, payload: Bytes) -> Self {
        PriorityFrameFacade {
            frame,
            payload,
            decoded_payload: None,
        }
    }

    pub fn decode_payload(mut payload: Bytes) -> anyhow::Result<PriorityFramePayload> {
        // Offset ahead 4 bytes.
        let raw = payload.get_u32();
        let exclusive = (raw & 0x8000_0000) != 0 ;
        let stream_dependency = (raw & 0x7FFF_FFFF);

        // Offset ahead 1 bytes.
        let weight = payload.get_u8();
        Ok(
            PriorityFramePayload{ exclusive, stream_dependency, weight }
        )
    }


    pub fn stream_dependency(&self) -> u32 {
        self.decoded_payload
            .as_ref()
            .expect("Unexpected situation. payload should be decoded before")
            .stream_dependency
    }

    pub fn exclusive(&self) -> bool {
        self.decoded_payload
            .as_ref()
            .expect("Unexpected situation. payload should be decoded before")
            .exclusive
    }

    pub fn weight(&self) -> u8 {
        self.decoded_payload
            .as_ref()
            .expect("Unexpected situation. payload should be decoded before")
            .weight
    }

}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.7
pub struct PriorityFrameHandler { }

impl PriorityFrameHandler {
    pub fn new() -> Self {
        PriorityFrameHandler {}
    }
}


impl FrameHandler for PriorityFrameHandler {

    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        let payload =  frame.payload();
        match frame {
            FrameFacade::PriorityFrame(f) => {
                f.decoded_payload = Some(PriorityFrameFacade::decode_payload(payload)?)
            },
            _ => ()
        }

        Ok(())
    }

    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        // #1
        //    The PRIORITY frame always identifies a stream.  If a PRIORITY frame
        //    is received with a stream identifier of 0x0, the recipient MUST
        //    respond with a connection error (Section 5.4.1) of type
        //    PROTOCOL_ERROR.
        if frame.stream_id() == 0 {
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
        }

        // #2
        //    A PRIORITY frame with a length other than 5 octets MUST be treated as
        //    a stream error (Section 5.4.2) of type FRAME_SIZE_ERROR.
        if frame.get_payload_size() != 5 {
            return Err(Http2Error::stream_error(frame.stream_id(), ErrorCode::FrameSizeError).into());
        }

        match frame {
            FrameFacade::PriorityFrame(f) => {
                // #3 (5.3.1 Stream Dependencies)
                //    A stream cannot depend on itself.  An endpoint MUST treat this as a
                //    stream error (Section 5.4.2) of type PROTOCOL_ERROR.
                if frame.stream_id() == f.stream_dependency() {
                    return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into())
                }
            },
            _ => ()
        };

        Ok(())

    }

    //    Receivers of a PING frame that does not include an ACK flag MUST send
    //    a PING frame with the ACK flag set in response, with an identical
    //    payload.  PING responses SHOULD be given higher priority than any
    //    other frame.
    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        Ok(None)
    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }
}