use bytes::{
    Buf,
    Bytes
};
use fluke_h2_parse::{
    DataFlags,
    Frame,
    FrameType
};
use fluke_h2_parse::enumflags2::BitFlag;
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
pub struct DataFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.1
impl DataFrameFacade {

    //
    //     +---------------+
    //     |Pad Length? (8)|
    //     +---------------+-----------------------------------------------+
    //     |                            Data (*)                         ...
    //     +---------------------------------------------------------------+
    //     |                           Padding (*)                       ...
    //     +---------------------------------------------------------------+
    //
    //                        Figure 6: DATA Frame Payload
    //
    //    The DATA frame contains the following fields:
    //
    //    Pad Length:  An 8-bit field containing the length of the frame
    //       padding in units of octets.  This field is conditional (as
    //       signified by a "?" in the diagram) and is only present if the
    //       PADDED flag is set.
    //
    //    Data:  Application data.  The amount of data is the remainder of the
    //       frame payload after subtracting the length of the other fields
    //       that are present.
    //     Padding:  Padding octets that contain no application semantic value.
    //       Padding octets MUST be set to zero when sending.  A receiver is
    //       not obligated to verify padding but MAY treat non-zero padding as
    //       a connection error (Section 5.4.1) of type PROTOCOL_ERROR.
    //
    //    The DATA frame defines the following flags:
    //
    //    END_STREAM (0x1):  When set, bit 0 indicates that this frame is the
    //       last that the endpoint will send for the identified stream.
    //       Setting this flag causes the stream to enter one of the "half-
    //       closed" states or the "closed" state (Section 5.1).
    //
    //    PADDED (0x8):  When set, bit 3 indicates that the Pad Length field
    //       and any padding that it describes are present.

    pub fn with(frame: Frame, payload: Bytes) -> Self {
        DataFrameFacade { frame, payload }
    }

    pub fn has_padded_flag(&self) -> bool {
        let flags = match self.frame.frame_type {
            FrameType::Data(f) => f,
            _ => DataFlags::empty()
        };

        flags.contains(DataFlags::Padded)
    }

    pub fn get_pad_length(&self) -> u8 {
        if self.has_padded_flag() {
            let mut b = Bytes::from(self.payload.clone());
            b.get_u8()
        } else {
          0
        }
    }

}




pub struct DataFrameHandler { }

impl DataFrameHandler {
    pub fn new() -> Self {
        DataFrameHandler {}
    }

}




impl FrameHandler for DataFrameHandler {

    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }

    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        // #1
        //    DATA frames MUST be associated with a stream.  If a DATA frame is
        //    received whose stream identifier field is 0x0, the recipient MUST
        //    respond with a connection error (Section 5.4.1) of type
        //    PROTOCOL_ERROR.
        if frame.stream_id() == 0 {
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
        };

        // #2
        //    The total number of padding octets is determined by the value of the
        //    Pad Length field.  If the length of the padding is the length of the
        //    frame payload or greater, the recipient MUST treat this as a
        //    connection error (Section 5.4.1) of type PROTOCOL_ERROR.
        match frame {
            FrameFacade::DataFrame(f) => {
                if f.has_padded_flag() && (f.get_pad_length() as usize > frame.get_payload_size()) {
                    return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
                }
            },
            // Unexpected Situation.
            _ => return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
        }

        Ok(())
    }

    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        Ok(None)
    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        conn_ctx.decrement_window_size_of_server(frame.get_payload_size());
        Ok(())
    }


}

impl TryInto<Bytes> for DataFrameFacade {

    type Error = anyhow::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        h2_frame_to_bytes(self.frame, &self.payload)
    }
}