use anyhow::bail;
use bytes::{Buf, Bytes};
use fluke_h2_parse::Frame;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http_connection_context::Http2ConnectionContext1;

#[derive(Debug)]
pub struct WindowUpdateFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
}

// RFC: https://datatracker.ietf.org/doc/html/rfc7540#section-6.9
impl WindowUpdateFrameFacade {

    //     +-+-------------------------------------------------------------+
    //     |R|              Window Size Increment (31)                     |
    //     +-+-------------------------------------------------------------+
    //
    //                   Figure 14: WINDOW_UPDATE Payload Format
    //    The WINDOW_UPDATE frame (type=0x8) is used to implement flow control;
    //    see Section 5.2 for an overview.
    //
    //    Flow control operates at two levels: on each individual stream and on
    //    the entire connection.
    //    1. connection level (stream_id = 0 )
    //    2. stream level. (stream_id != 0)

    //  ### About Flow Control. (Only DataFrame is under the control)
    //   Flow control only applies to frames that are identified as being
    //    subject to flow control.  Of the frame types defined in this
    //    document, this includes only DATA frames.  Frames that are exempt
    //    from flow control MUST be accepted and processed, unless the receiver
    //    is unable to assign resources to handling the frame.

    //  Error Case
    // increment = 0 => for the stream PROTOCOL_ERROR (stream error), for the connection connection error


    // Possible State
    // This means that a receiver could receive a
    //    WINDOW_UPDATE frame on a "half-closed (remote)" or "closed" stream.

    pub fn with(frame: Frame, payload: Bytes) -> Self {
        WindowUpdateFrameFacade { frame, payload }
    }

    pub fn decode_payload(mut payload: Bytes) -> anyhow::Result<u32> {
        if payload.len() != 4 {
            bail!("WINDOW_UPDATE payload must be 4 bytes, got {}", payload.len())
        }

        let raw = payload.get_u32();
        // bit masking
        // 0111 1111 , 1111 1111, 1111 1111, 1111 1111
        // & = 'AND' Operation
        let increment = raw & 0x7FFF_FFFF;

        Ok(increment)
    }

    pub fn decode_payload_from(&self) -> anyhow::Result<u32> {
        let raw = self.payload.clone().get_u32();
        let increment = raw & 0x7FFF_FFFF;
        Ok(increment)
    }

    // pub fn decode_payload_from1(&self) -> anyhow::Result<u32> {
    //     let raw = self.payload.get_u32();
    //     let increment = raw & 0x7FFF_FFFF;
    //     Ok(increment)
    // }

    pub fn get_payload(&self) -> Bytes {
        self.payload.clone()
    }
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.7
pub struct WindowUpdateFrameHandler { }

impl WindowUpdateFrameHandler {
    pub fn new() -> Self {
        WindowUpdateFrameHandler {}
    }
}


impl FrameHandler for WindowUpdateFrameHandler {


    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }

    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{

        //    A receiver MUST treat the receipt of a WINDOW_UPDATE frame with an
        //    flow-control window increment of 0 as a stream error (Section 5.4.2)
        //    of type PROTOCOL_ERROR; errors on the connection flow-control window
        //    MUST be treated as a connection error (Section 5.4.1).
        match frame {
            FrameFacade::WindowUpdateFrame(f) => {
                let increment = f.decode_payload_from()?;
                let sid = frame.stream_id();

                // "sid == 0" is for connection level window size.
                if sid == 0 && increment == 0 {
                    return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into())
                }

                if sid != 0 && increment == 0 {
                    return Err(Http2Error::stream_error(sid, ErrorCode::ProtocolError).into())
                }

                let exceed = (increment as u64) + conn_ctx.current_window_size() as u64;

                //    A sender MUST NOT allow a flow-control window to exceed 2^31-1
                //    octets.  If a sender receives a WINDOW_UPDATE that causes a flow-
                //    control window to exceed this maximum, it MUST terminate either the
                //    stream or the connection, as appropriate.  For streams, the sender
                //    sends a RST_STREAM with an error code of FLOW_CONTROL_ERROR; for the
                //    connection, a GOAWAY frame with an error code of FLOW_CONTROL_ERROR
                //    is sent.
                let is_connection_window_exceed = exceed > u64::pow(2, 32) - 1;
                if sid == 0 &&  is_connection_window_exceed {
                    return Err(Http2Error::ConnectionError(ErrorCode::FlowControlError).into())
                }
            }
            _ => ()
        }

        //    A WINDOW_UPDATE frame with a length other than 4 octets MUST be
        //    treated as a connection error (Section 5.4.1) of type
        //    FRAME_SIZE_ERROR.
        if frame.payload().len() != 4 {
            return Err(Http2Error::ConnectionError(ErrorCode::FrameSizeError).into());
        }

        Ok(())

    }

    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        // Frames with zero length with the END_STREAM flag set (that
        //    is, an empty DATA frame) MAY be sent if there is no available space
        //    in either flow-control window.

        Ok(None)
    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        match frame {
            FrameFacade::WindowUpdateFrame(f) => {
                let sid = frame.stream_id();

                // "sid == 0" is for connection level window size.
                if sid == 0 {
                    let increment = f.decode_payload_from()?;
                    conn_ctx.increment_window_size(increment)
                }
            }
            _ => ()
        }

        Ok(())
    }
}