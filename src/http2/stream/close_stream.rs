use std::collections::{HashMap, HashSet, VecDeque};
use tokio_util::sync::CancellationToken;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::{ContinuationFrame, DataFrame, GoawayFrame, HeadersFrame, PingFrame, PriorityFrame, RstStreamFrame, WindowUpdateFrame};
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http2::http2_stream::{DecodingHeaderState, StreamState};
use crate::http2::stream::stream::{Stream, StreamInterface};
use crate::http2::stream::stream_context::StreamContext;

pub struct CloseStreamInner {
    // stream_id: u32,
    // decoding_header_state: DecodingHeaderState,
    // decoding_trailers_state: DecodingHeaderState,
    //
    // window_about_client : u32,
    // window_about_me     : u32,
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

impl StreamInterface for CloseStreamInner {

    // fn decoding_header_state(&self) -> DecodingHeaderState {
    //     self.decoding_header_state
    // }
    //
    // fn decoding_trailers_state(&self) -> DecodingHeaderState {
    //     self.decoding_trailers_state
    // }

    fn maybe_next_state_from_client(self, frame: &FrameFacade, ctx: StreamContext) -> anyhow::Result<Stream> {
        match frame {

            // An endpoint MUST NOT send frames other than PRIORITY on a closed
            // stream.
            PriorityFrame(_) => Ok(Stream::ClosedStream(self, ctx)),

            // Endpoints MUST ignore WINDOW_UPDATE or RST_STREAM frames received in this state, though
            // endpoints MAY choose to treat frames that arrive a significant
            // time after sending END_STREAM as a connection error
            // (Section 5.4.1) of type PROTOCOL_ERROR.
            RstStreamFrame(_) => Ok(Stream::ClosedStream(self, ctx)),
            WindowUpdateFrame(_) => Ok(Stream::ClosedStream(self, ctx)),

            // *** Closed
            // An endpoint that receives any frame other than PRIORITY
            // after receiving a RST_STREAM MUST treat that as a stream error
            // (Section 5.4.2) of type STREAM_CLOSED.
            // WINDOW_UPDATE or RST_STREAM frames can be received in this state
            // for a short period after a DATA or HEADERS frame containing an
            // END_STREAM flag is sent.  Until the remote peer receives and
            // processes RST_STREAM or the frame bearing the END_STREAM flag, it
            // might send frames of these types.  Endpoints MUST ignore
            // WINDOW_UPDATE or RST_STREAM frames received in this state, though
            // endpoints MAY choose to treat frames that arrive a significant
            // time after sending END_STREAM as a connection error
            // (Section 5.4.1) of type PROTOCOL_ERROR.
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

            _ => Err(
                Http2Error::stream_error(ctx.stream_id(), ErrorCode::StreamClosed).into()
            ),
        }
    }

    fn maybe_next_state_from_me(self, frame: &FrameFacade, ctx: StreamContext)
                                -> anyhow::Result<Stream>
    {
        match frame {
            _                                                => Ok(Stream::ClosedStream(self.into(), ctx)),
        }
    }


}
