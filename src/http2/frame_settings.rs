use anyhow::bail;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use fluke_h2_parse::enumflags2::BitFlags;
use fluke_h2_parse::{Frame, SettingsFlags, StreamId};
use fluke_h2_parse::FrameType::Settings;
use tracing::debug;
use crate::http2::common_frame_facade::h2_frame_to_bytes;
use crate::http2::common_frame_handler::FrameHandler;
use crate::http2::http2_conn_options::PartialHttp2ConnectionOptions;
use crate::http2::common_frame_facade::{FrameFacade};
use crate::http2::common_frame_facade::FrameFacade::SettingsFrame;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http_connection_context::Http2ConnectionContext1;



#[derive(Debug)]
pub struct SettingsFrameFacade {
    pub(crate) frame: Frame,
    pub(crate) payload: Bytes,
    pub(crate) decoded_payload: Option<Vec<(u16, u32)>>
}

impl SettingsFrameFacade {

    pub fn is_ack(&self) -> bool {
        match self.frame.frame_type {
            Settings(f) => f.contains(SettingsFlags::Ack),
            _ => false
        }
    }

}

#[repr(u16)]
#[derive(Debug, Clone, Copy)]
enum SettingsPayloadStreamId {
    HeaderTableSize = 0x01, // 16bits => 0000 0001
    EnablePush = 0x02, // 16bits => 0000 0002
    MaxConcurrentStreams = 0x03, // 16bits => 0000 0003
    InitialWindowSize = 0x04, // 16bits => 0000 0004
    MaxFrameSize = 0x05, // 16bits => 0000 0005
    MaxHeaderListSize = 0x06, // 16bits => 0000 0006
}

pub enum SettingsParameter {
    HeaderTableSize(u64),
    EnablePush(u64),
    MaxConcurrentStreams(u64),
    InitialWindowSize(u64),
    MaxFrameSize(u64),
    MaxHeaderListSize(u64),
}


impl From<SettingsPayloadStreamId> for u16 {
    fn from(value: SettingsPayloadStreamId) -> Self {
        value as u16
    }
}

// https://datatracker.ietf.org/doc/html/rfc7540#section-6.5.1
impl SettingsFrameFacade {

    //    The payload of a SETTINGS frame consists of zero or more parameters,
    //    each consisting of an unsigned 16-bit setting identifier and an
    //    unsigned 32-bit value.
    //
    //     +-------------------------------+
    //     |       Identifier (16)         |
    //     +-------------------------------+-------------------------------+
    //     |                        Value (32)                             |
    //     +---------------------------------------------------------------+
    pub fn new(payload: Bytes) -> Self {
        let frame = Frame::new(Settings(BitFlags::empty()), StreamId(0));
        SettingsFrameFacade { frame, payload, decoded_payload: None }
    }

    pub fn with(frame: Frame, payload: Bytes) -> Self {
        SettingsFrameFacade { frame, payload, decoded_payload: None}
    }

    pub fn default_payload() -> Vec<(u16, u32)>{
        vec![
            (SettingsPayloadStreamId::HeaderTableSize.into(), 4096 as u32), // 4096 octets = 4096 bytes
            (SettingsPayloadStreamId::EnablePush.into(), 1 as u32),
            (SettingsPayloadStreamId::MaxConcurrentStreams.into(), 100 as u32), // Recommend value by rfc.
            (SettingsPayloadStreamId::InitialWindowSize.into(), 65535 as u32),
            (SettingsPayloadStreamId::MaxFrameSize.into(), 16384 as u32),
            (SettingsPayloadStreamId::MaxHeaderListSize.into(), 0x01),
        ]
    }

    pub fn encode_payload(mut payload: Vec<(u16, u32)>) -> anyhow::Result<Bytes> {
        let mut buf = BytesMut::with_capacity(payload.len() * 6);
        for (id, val) in payload {
            buf.put_u16(id);
            buf.put_u32(val);
        };
        Ok(buf.freeze())
    }

    pub fn decode_payload_from(&mut self) -> anyhow::Result<&Vec<(u16, u32)>> {
        if self.decoded_payload.is_none() {
            self.decoded_payload = Some(Self::decode_payload(self.payload.clone())?);
        }
        Ok(self.decoded_payload.as_ref().unwrap())
    }

    pub fn window_size(&mut self) -> anyhow::Result<u32> {
        if let Some(ref key_pair) = self.decoded_payload {
            for (k ,v) in key_pair {
                return match k {
                    4 => Ok(*v),
                    _ => Ok(65535)
                }
            }
        }

        Ok(65535)
    }

    pub fn decode_payload(mut payload: Bytes) -> anyhow::Result<Vec<(u16, u32)>> {
        // HeaderTableSize = 0x01, // 16bits => 0000 0001
        // EnablePush = 0x02, // 16bits => 0000 0002
        // MaxConcurrentStreams = 0x03, // 16bits => 0000 0003
        // InitialWindowSize = 0x04, // 16bits => 0000 0004
        // MaxFrameSize = 0x05, // 16bits => 0000 0005
        // MaxHeaderListSize = 0x06, // 16bits => 0000 0006

        // id :  2 Byte
        // value : 4 Byte
        let mut out = Vec::with_capacity(payload.len() / 6);
        while payload.remaining() >= 6 {
            let id = payload.get_u16();  // big-endian
            let val = payload.get_u32(); // big-endian
            out.push((id, val));
        }

        Ok(out)
    }

    pub fn raw_options(&self) -> Vec<SettingsParameter> {
        let mut payload = self.payload.clone();
        let mut raw_options = Vec::with_capacity(payload.len() / 6);


        while payload.remaining() >= 6 {
            let id = payload.get_u16();  // big-endian
            let v = payload.get_u32() as u64; // big-endian
            match id {
                0x1 => raw_options.push(SettingsParameter::HeaderTableSize(v)),
                0x2 => raw_options.push(SettingsParameter::EnablePush(v)),
                0x3 => raw_options.push(SettingsParameter::MaxConcurrentStreams(v)),
                0x4 => raw_options.push(SettingsParameter::InitialWindowSize(v)),
                0x5 => raw_options.push(SettingsParameter::MaxFrameSize(v)),
                0x6 => raw_options.push(SettingsParameter::MaxHeaderListSize(v)),
                //    An endpoint that receives a SETTINGS frame with any unknown or
                //    unsupported identifier MUST ignore that setting.
                _ => ()
            }
        }

        raw_options
    }

    pub fn options(&self) -> PartialHttp2ConnectionOptions {
        let mut options = PartialHttp2ConnectionOptions::new();

        for (k ,v) in Self::decode_payload(self.payload.clone()).unwrap() {
            match k {
                1 => {
                    options.header_table_size = Some(v);
                },
                2 => {
                    // 1: enable, 0: disable.
                    let value = if v == 1 { true } else { false };
                    options.enable_push = Some(value);
                },
                3 => {
                    options.max_concurrent_streams = Some(v);
                },
                4 => {
                    options.initial_window_size = Some(v);
                }
                5 => {
                    options.max_frame_size = Some(v);
                }
                6 => {
                    options.max_header_list_size = Some(v);
                },
                _ => {
                    //    An endpoint that receives a SETTINGS frame with any unknown or
                    //    unsupported identifier MUST ignore that setting.
                    debug!("Unknown Options key: {}, value: {}", k, v);
                }
            }
        };
        options
    }

}


pub struct SettingsFrameHandler { }

impl SettingsFrameHandler {
    pub fn new() -> Self {
        SettingsFrameHandler {}
    }
}


impl FrameHandler for SettingsFrameHandler {

    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()> {
        Ok(())
    }

    // RFC : https://datatracker.ietf.org/doc/html/rfc7540#section-6.5
    // SETTINGS frames always apply to a connection, never a single stream.
    // The stream identifier for a SETTINGS frame MUST be zero (0x0).


    // The SETTINGS frame affects connection state.
    // SETTINGS Frame = [Header: 9 bytes] + [payload]
    // each payload : identifier 16 bits(2byte) + value 32 bits(4bytes)


    // The payload of a SETTINGS frame consists of zero or more parameters,
    // each consisting of an unsigned 16-bit setting identifier and an
    // unsigned 32-bit value.
    //
    // +-------------------------------+
    // |       Identifier (16)         |
    // +-------------------------------+-------------------------------+
    // |                        Value (32)                             |
    // +---------------------------------------------------------------+
    //
    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        // The stream identifier for a SETTINGS frame MUST be zero (0x0).  If an
        // endpoint receives a SETTINGS frame whose stream identifier field is
        // anything other than 0x0, the endpoint MUST respond with a connection
        // error (Section 5.4.1) of type PROTOCOL_ERROR.
        if frame.stream_id() != 0 {
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
        }

        // A SETTINGS frame with a length other than a multiple of 6 octets MUST
        // be treated as a connection error (Section 5.4.1) of type
        // FRAME_SIZE_ERROR.
        if frame.get_payload_size() % 6 != 0 {
            return Err(Http2Error::connection_error(ErrorCode::FrameSizeError).into());
        }

        match frame {
            FrameFacade::SettingsFrame(f) => {
                for option in f.raw_options() {
                    match option {
                        SettingsParameter::HeaderTableSize(v ) => {

                        },
                        SettingsParameter::EnablePush(v) => {
                            //       The initial value is 1, which indicates that server push is
                            //       permitted.  Any value other than 0 or 1 MUST be treated as a
                            //       connection error (Section 5.4.1) of type PROTOCOL_ERROR.
                            if v > 1 {
                                return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
                            }

                        },
                        SettingsParameter::MaxConcurrentStreams(v) => (),
                        SettingsParameter::InitialWindowSize(v) => {
                            // Values above the maximum flow-control window size of 2^31-1 MUST
                            // be treated as a connection error (Section 5.4.1) of type
                            // FLOW_CONTROL_ERROR.
                            if v > (2u64.pow(31) - 1) {
                                return Err(Http2Error::connection_error(ErrorCode::FlowControlError).into());
                            }
                        },
                        SettingsParameter::MaxFrameSize(v) => {
                            // SETTINGS_MAX_FRAME_SIZE (0x05):
                            // This setting indicates the size of the largest frame payload
                            // that the sender is willing to receive, in units of octets.
                            // The initial value is 214 (16,384) octets. The value advertised
                            // by an endpoint MUST be between this initial value and the maximum allowed frame size
                            // (2^24-1 or 16,777,215 octets), inclusive. Values outside this range MUST be treated as
                            //  a connection error (Section 5.4.1) of type PROTOCOL_ERROR.
                            if v > (2u64.pow(24) - 1) || v < 16384 {
                                return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
                            }
                        },
                        SettingsParameter::MaxHeaderListSize(v) => (),
                    }
                }
            },
            // This is not a spec of RFC. just unexpected situation.
            _ => return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into())
        }

        Ok(())
    }

    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        let is_ack = match frame {
            SettingsFrame(f) => {
                f.is_ack()
            }
            // unexpected.
            _ => return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
        };

        if !is_ack {
            let flags = BitFlags::from(SettingsFlags::Ack);
            let stream_id = StreamId(frame.stream_id());
            let ack_frame = Frame::new(Settings(flags), stream_id);

            Ok(Some(SettingsFrame(SettingsFrameFacade::with(ack_frame, Bytes::new()))))
        } else {
            Ok(None)
        }
    }

    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()> {
        match frame {
            SettingsFrame( ref facade) => {
                conn_ctx.update_options_from_client(facade.options())
            },
            _ => {
                debug!("frame handler got unexpected frame. it expects settings frame, but actual : {:?}", frame);
            }
        }

        Ok(())
    }
}


impl TryInto<Bytes> for SettingsFrameFacade {

    type Error = anyhow::Error;

    fn try_into(self) -> Result<Bytes, Self::Error> {
        let mut bytes = BytesMut::new();
        let payload = match self.decoded_payload {
            Some(p) => p,
            None => Self::decode_payload(self.payload.clone())?,
        };

        for (id, value) in payload.iter() {
            bytes.put_u16(*id);
            bytes.put_u32(*value);
        }
        let b = bytes.freeze();
        h2_frame_to_bytes(self.frame, &b)
    }
}