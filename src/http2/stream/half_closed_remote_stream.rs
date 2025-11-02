use std::collections::{HashMap, HashSet, VecDeque};
use tokio_util::sync::CancellationToken;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::{ContinuationFrame, DataFrame, GoawayFrame, HeadersFrame, PingFrame, PriorityFrame, RstStreamFrame, WindowUpdateFrame};
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http2::http2_stream::{DecodingHeaderState, StreamState};
use crate::http2::stream::close_stream::CloseStreamInner;
use crate::http2::stream::stream::{Stream, StreamInterface};
use crate::http2::stream::stream_context::StreamContext;
pub struct HalfClosedRemoteStreamInner {
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

impl StreamInterface for HalfClosedRemoteStreamInner {

    // fn decoding_header_state(&self) -> DecodingHeaderState {
    //     self.decoding_header_state
    // }
    //
    // fn decoding_trailers_state(&self) -> DecodingHeaderState {
    //     self.decoding_trailers_state
    // }

    fn maybe_next_state_from_client(self, frame: &FrameFacade, ctx: StreamContext) -> anyhow::Result<Stream> {
        match frame {
            RstStreamFrame(_)       => Ok(Stream::ClosedStream(self.into(), ctx)),
            // Don't change state. just for explicit.
            ContinuationFrame(_)    => Ok(Stream::HalfClosedRemoteStream(self.into(), ctx)),
            // HalfClosedRemote -> Priority Frame, Continuation Frame
            PriorityFrame(_) => Ok(Stream::HalfClosedRemoteStream(self.into(), ctx)),
            WindowUpdateFrame(_) => Ok(Stream::HalfClosedRemoteStream(self.into(), ctx)),

            // *** Half-closed (Remote)
            //    If an endpoint receives additional frames, other than
            //    WINDOW_UPDATE, PRIORITY, or RST_STREAM, for a stream that is in
            //    this state, it MUST respond with a stream error (Section 5.4.2) of
            //    type STREAM_CLOSED.
            DataFrame(_) => Err(
                Http2Error::stream_error(ctx.stream_id(), ErrorCode::StreamClosed).into()
            ),
            HeadersFrame(_) => Err(
                Http2Error::stream_error(ctx.stream_id(), ErrorCode::StreamClosed).into()
            ),
            PingFrame(_) => Err(
                Http2Error::stream_error(ctx.stream_id(), ErrorCode::StreamClosed).into()
            ),
            GoawayFrame(_) => Err(
                Http2Error::stream_error(ctx.stream_id(), ErrorCode::StreamClosed).into()
            ),

            _             => Err(
                Http2Error::stream_error(ctx.stream_id(), ErrorCode::StreamClosed).into()
            ),
        }
    }

    fn maybe_next_state_from_me(self, frame: &FrameFacade, ctx: StreamContext)
                                -> anyhow::Result<Stream>
    {
        match frame {
            HeadersFrame(_) if frame.has_end_stream_flag()         => Ok(Stream::ClosedStream(self.into(), ctx)),
            DataFrame(_) if frame.has_end_stream_flag()            => Ok(Stream::ClosedStream(self.into(), ctx)),
            RstStreamFrame(_)                                      => Ok(Stream::ClosedStream(self.into(), ctx)),
            _                                                      => Ok(Stream::HalfClosedRemoteStream(self.into(), ctx)),
        }
    }


}


impl From<HalfClosedRemoteStreamInner> for CloseStreamInner {
    fn from(value: HalfClosedRemoteStreamInner) -> Self {
        todo!()
    }
}