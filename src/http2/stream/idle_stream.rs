use std::collections::{HashMap, HashSet, VecDeque};
use anyhow::anyhow;
use tokio_util::sync::CancellationToken;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::{ContinuationFrame, DataFrame, GoawayFrame, HeadersFrame, PingFrame, PriorityFrame, RstStreamFrame, WindowUpdateFrame};
use crate::http2::http2_errors::{
    ErrorCode,
    Http2Error
};
use crate::http2::http2_stream::{DecodingHeaderState, StreamState};
use crate::http2::stream::half_closed_local_stream::HalfClosedLocalStreamInner;
use crate::http2::stream::half_closed_remote_stream::HalfClosedRemoteStreamInner;
use crate::http2::stream::open_stream::OpenStreamInner;
use crate::http2::stream::stream::{Stream, StreamInterface};
use crate::http2::stream::stream_context::StreamContext;

pub struct IdleStreamInner {
    // stream_id: u32,
    // decoding_header_state: DecodingHeaderState,
    // decoding_trailers_state: DecodingHeaderState,
    //
    // // Inflight Headers & Body
    // inflight_headers: VecDeque<FrameFacade>,
    //
    // maybe_trailers: Option<HashSet<String>>,
    // inflight_trailers_headers: VecDeque<FrameFacade>,
    //
    // composited_header: Option<HashMap<String, String>>,
    //
    // inflight_bodies: VecDeque<FrameFacade>,
    //
    // // For Graceful shutdown
    // cancellation_token: CancellationToken,
    //
    // // Because of window size,
    // pending_frames: VecDeque<FrameFacade>,
    //
    // // Flags
    // // inflight_headers_end: bool,
    // request_end: bool,
    //
    // should_terminate_stream: bool,
    //
    // dispatched: bool,
    //
    // // http2_request_context: Option<Http2RequestContext>,
    // response_completed: bool,
    //
    // // Stream -> Client
    // send_end_headers_completed: bool,
    // send_end_stream_completed: bool,
    //
    // error: Option<Http2Error>,
}

impl StreamInterface for IdleStreamInner {

    // fn decoding_header_state(&self) -> DecodingHeaderState {
    //     self.decoding_header_state
    // }
    //
    // fn decoding_trailers_state(&self) -> DecodingHeaderState {
    //     self.decoding_trailers_state
    // }

    fn maybe_next_state_from_client(self, frame: &FrameFacade, ctx: StreamContext) -> anyhow::Result<Stream> {
        match frame {
            HeadersFrame(_) if frame.has_end_stream_flag()     => Ok(Stream::HalfClosedRemoteStream(self.into(), ctx)),
            HeadersFrame(_)                                    => Ok(Stream::OpenStream(self.into(), ctx)),
            PriorityFrame(_)                                   => Ok(Stream::IdleStream(self.into(), ctx)),

            // **** Validate Fail.
            //    RST_STREAM frames MUST NOT be sent for a stream in the "idle" state.
            //    If a RST_STREAM frame identifying an idle stream is received, the
            //    recipient MUST treat this as a connection error (Section 5.4.1) of
            //    type PROTOCOL_ERROR.  (https://datatracker.ietf.org/doc/html/rfc7540#section-6.4)
            RstStreamFrame(_) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),

            // *** IDLE
            //       Receiving any frame other than HEADERS or PRIORITY on a stream in
            //       this state MUST be treated as a connection error (Section 5.4.1)
            //       of type PROTOCOL_ERROR.
            DataFrame(_) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            PingFrame(_) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            ContinuationFrame(_) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            GoawayFrame(_) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            WindowUpdateFrame(_) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),

            _ => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
        }
    }

    fn maybe_next_state_from_me(self, frame: &FrameFacade, ctx: StreamContext)
        -> anyhow::Result<Stream>
    {
        match frame {
            HeadersFrame(_) if frame.has_end_stream_flag()     => Ok(Stream::HalfClosedLocalStream(self.into(), ctx)),
            HeadersFrame(_)                                    => Ok(Stream::OpenStream(self.into(), ctx)),
            PriorityFrame(_)                                   => Ok(Stream::IdleStream(self.into(), ctx)),
            _                                                  => Ok(Stream::IdleStream(self.into(), ctx)),

        }
    }


}


impl From<IdleStreamInner> for OpenStreamInner {
    fn from(inner: IdleStreamInner) -> Self {
        todo!()
    }
}

impl From<IdleStreamInner> for HalfClosedLocalStreamInner {

    fn from(inner: IdleStreamInner) -> Self {
        todo!()
    }
}

impl From<IdleStreamInner> for HalfClosedRemoteStreamInner {
    fn from(value: IdleStreamInner) -> Self {
        todo!()
    }
}