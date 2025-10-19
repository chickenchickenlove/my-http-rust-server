use anyhow::{
    anyhow,
    bail
};
use bytes::{
    Buf,
    Bytes
};
use fluke_h2_parse::{
    Frame,
    FrameType,
    HeadersFlags
};
use fluke_h2_parse::enumflags2::{
    BitFlag,
    BitFlags
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
pub struct HeadersFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
    pub(crate) parameters: Option<HeaderFrameParameter>,
}


#[derive(Debug)]
pub struct HeaderFrameParameter {
    pad_length: Option<u8>,
    exclusive: Option<bool>,
    stream_dependency: Option<u32>,
    weight: Option<u16>,
    header_block_fragment: Option<Bytes>,
}

impl HeaderFrameParameter {

    pub fn new() -> Self {
        HeaderFrameParameter {
            pad_length: None,
            exclusive: None,
            stream_dependency: None,
            weight: None,
            header_block_fragment: None,
        }
    }


}


// https://datatracker.ietf.org/doc/html/rfc7540#section-6.2
impl HeadersFrameFacade {

    //     +---------------+
    //     |Pad Length? (8)|
    //     +-+-------------+-----------------------------------------------+
    //     |E|                 Stream Dependency? (31)                     |
    //     +-+-------------+-----------------------------------------------+
    //     |  Weight? (8)  |
    //     +-+-------------+-----------------------------------------------+
    //     |                   Header Block Fragment (*)                 ...
    //     +---------------------------------------------------------------+
    //     |                           Padding (*)                       ...
    //     +---------------------------------------------------------------+
    //
    //                       Figure 7: HEADERS Frame Payload
    //
    //    The HEADERS frame payload has the following fields:
    //
    //    Pad Length:  An 8-bit field containing the length of the frame
    //       padding in units of octets.  This field is only present if the
    //       PADDED flag is set.
    //
    //    E: A single-bit flag indicating that the stream dependency is
    //       exclusive (see Section 5.3).  This field is only present if the
    //       PRIORITY flag is set.
    //
    //    Stream Dependency:  A 31-bit stream identifier for the stream that
    //       this stream depends on (see Section 5.3).  This field is only
    //       present if the PRIORITY flag is set.
    //
    //    Weight:  An unsigned 8-bit integer representing a priority weight for
    //       the stream (see Section 5.3).  Add one to the value to obtain a
    //       weight between 1 and 256.  This field is only present if the
    //       PRIORITY flag is set.
    //
    //    Header Block Fragment:  A header block fragment (Section 4.3).
    //
    //    Padding:  Padding octets.

    // The HEADERS frame defines the following flags:
    //
    //    END_STREAM (0x1):  When set, bit 0 indicates that the header block
    //       (Section 4.3) is the last that the endpoint will send for the
    //       identified stream.
    //
    //       A HEADERS frame carries the END_STREAM flag that signals the end
    //       of a stream.  However, a HEADERS frame with the END_STREAM flag
    //       set can be followed by CONTINUATION frames on the same stream.
    //       Logically, the CONTINUATION frames are part of the HEADERS frame.
    //    END_HEADERS (0x4):  When set, bit 2 indicates that this frame
    //       contains an entire header block (Section 4.3) and is not followed
    //       by any CONTINUATION frames.
    //
    //       A HEADERS frame without the END_HEADERS flag set MUST be followed
    //       by a CONTINUATION frame for the same stream.  A receiver MUST
    //       treat the receipt of any other type of frame or a frame on a
    //       different stream as a connection error (Section 5.4.1) of type
    //       PROTOCOL_ERROR.
    //
    //    PADDED (0x8):  When set, bit 3 indicates that the Pad Length field
    //       and any padding that it describes are present.
    //
    //    PRIORITY (0x20):  When set, bit 5 indicates that the Exclusive Flag
    //       (E), Stream Dependency, and Weight fields are present; see
    //       Section 5.3.


    pub fn with(frame: Frame, payload: Bytes) -> Self {
        HeadersFrameFacade { frame, payload, parameters: None }
    }


    fn decode_payload_and_value(flags: BitFlags<HeadersFlags>, mut payload: Bytes) -> anyhow::Result<HeaderFrameParameter> {
        let mut paramters = HeaderFrameParameter::new();

        let mut pad_len = 0usize;

        if flags.contains(HeadersFlags::Padded) {
            let padded = payload.get_u8();
            paramters.pad_length = Some(padded);
            pad_len = padded as usize;
        }

        if flags.contains(HeadersFlags::Priority) {
            let priority_raw = payload.get_u32();

            let exclusive = (priority_raw & 0x8000_0000) != 0; // 0 => not exclusive, other => exclusive.
            let stream_dependency = priority_raw & 0x7FFF_FFFF;
            let weight = payload.get_u8() as u16 + 1; // 1..256

            paramters.exclusive = Some(exclusive);
            paramters.stream_dependency = Some(stream_dependency);
            paramters.weight = Some(weight);
        }

        let headers_fragment_len = payload.len() - pad_len;
        let headers_fragment = payload.copy_to_bytes(headers_fragment_len);

        paramters.header_block_fragment = Some(headers_fragment);

        Ok(paramters)
    }


    pub fn get_header_fragment(&self) -> anyhow::Result<Bytes> {
        let parameter = self.parameters
            .as_ref()
            .expect("Unexpected empty parameters");
        Ok(parameter.header_block_fragment
            .as_ref()
            .expect("Unexpected empty header block fragment")
            .clone())
    }

    pub fn get_stream_dependency(&self) -> Option<u32> {
        let parameter = self.parameters
            .as_ref()
            .expect("Unexpected empty parameters");

        parameter.stream_dependency.clone()
    }

    pub fn get_header_flags(&self) -> BitFlags<HeadersFlags> {
        match self.frame.frame_type {
            FrameType::Headers(flags) => flags,
            _ => BitFlags::empty()
        }
    }


    pub fn is_end_headers(&self) -> bool {
        self.get_header_flags()
            .contains(HeadersFlags::EndHeaders)
    }

    pub fn is_received_end_stream(&self) -> bool {
        self.get_header_flags()
            .contains(HeadersFlags::EndStream)
    }

}


pub struct HeadersFrameHandler { }

impl HeadersFrameHandler {
    pub fn new() -> Self {
        HeadersFrameHandler {}
    }


    fn validate_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>
    {
        if let Err(e) = match frame {
            FrameFacade::HeadersFrame(f) => f.get_header_fragment(),
            _ => Err(anyhow!("Unexpected header type.")),
        } {
            return Err(Http2Error::connection_error(ErrorCode::CompressionError).into())
        };

        match frame {
            FrameFacade::HeadersFrame(f) => {
                if let Some(stream_dependency) = f.get_stream_dependency() {
                    // 5.3.1. Stream Dependencies
                    //    A stream cannot depend on itself.  An endpoint MUST treat this as a
                    //    stream error (Section 5.4.2) of type PROTOCOL_ERROR.
                    if stream_dependency == frame.stream_id() {
                        return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into())
                    }
                }
            },
            _ => bail!("Unexpected header type."),
        }
        Ok(())
    }

}


impl FrameHandler for HeadersFrameHandler {


    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        let payload = frame.payload();
        match frame {
            FrameFacade::HeadersFrame(ref mut facade) => {
                facade.parameters = Some(
                    HeadersFrameFacade::decode_payload_and_value(facade.get_header_flags(), payload)?
                );
            },
            _ => ()
        }


        Ok(())
    }

    //    HEADERS frames MUST be associated with a stream.  If a HEADERS frame
    //    is received whose stream identifier field is 0x0, the recipient MUST
    //    respond with a connection error (Section 5.4.1) of type
    //    PROTOCOL_ERROR.
    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        let sid = frame.stream_id();
        if sid == 0 {
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into())
        }

        println!("conn_ctx.get_max_concurrent_streams_from_me() > conn_ctx.get_active_streams_count() + 1 : {}", conn_ctx.get_max_concurrent_streams_from_me() > conn_ctx.get_active_streams_count() + 1);
        if conn_ctx.get_active_streams_count() + 1 > conn_ctx.get_max_concurrent_streams_from_me() {
            return Err(
                Http2Error::stream_error(sid, ErrorCode::RefusedStream).into()
            )
        }

        self.validate_payload(conn_ctx, frame)?;

        Ok(())
    }

    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        Ok(None)
    }

    // The HEADERS frame changes the connection state as described in
    // Section 4.3.
    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        let sid = frame.stream_id();

        if !conn_ctx.has_inflight_headers() {
            if !frame.has_end_headers_flag() {
                conn_ctx.update_inflight_headers(frame.stream_id())
            }
        }

        Ok(())
    }
}

impl TryInto<Bytes> for HeadersFrameFacade {

    type Error = anyhow::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        h2_frame_to_bytes(self.frame, &self.payload)
    }
}