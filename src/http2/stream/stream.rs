use std::collections::{HashMap, HashSet, VecDeque};
use tokio_util::sync::CancellationToken;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http2::http2_response::Http2Response;
use crate::http2::http2_stream::{DecodingHeaderState, StreamResult};
use crate::http2::stream::close_stream::CloseStreamInner;
use crate::http2::stream::half_closed_local_stream::HalfClosedLocalStreamInner;
use crate::http2::stream::half_closed_remote_stream::HalfClosedRemoteStreamInner;
use crate::http2::stream::idle_stream::IdleStreamInner;
use crate::http2::stream::open_stream::OpenStreamInner;
use crate::http2::stream::stream::Stream::IdleStream;
use crate::http2::stream::stream_context::StreamContext;
use crate::http_connection_context::Http2ConnectionContext1;
use crate::http_request_context::Http2RequestContext;

pub trait StreamInterface {

    // async fn send_request_ctx(&mut self, request_ctx: Http2RequestContext) -> anyhow::Result<()>;
    // async fn maybe_composite_headers(&mut self) -> anyhow::Result<()>;
    // async fn maybe_composite_trailers(&mut self) -> anyhow::Result<()>;
    // async fn handle_response_object(&mut self, response: Http2Response) -> anyhow::Result<StreamResult>;
    //
    // fn handle_trailers_in_progress(&self) -> bool;
    // fn handle_frame_for_request(&mut self, frame: FrameFacade) -> anyhow::Result<StreamResult>;
    // async fn maybe_dispatch_if_needed(&mut self) -> anyhow::Result<()>;
    // async fn response_to_client_if_possible(&mut self) -> anyhow::Result<()>;
    // async fn serve(mut self) -> anyhow::Result<()>;
    // fn flush(&mut self) -> Vec<FrameFacade>;
    // fn can_send_frame(&mut self, payload_size: usize);
    // fn validate_from_client(&mut self, frame: &FrameFacade) -> anyhow::Result<()>;
    //
    // fn validate_from_me(&mut self, frame: &FrameFacade) -> anyhow::Result<()>;
    //
    // fn decoding_header_state(&self) -> DecodingHeaderState;
    // fn decoding_trailers_state(&self) -> DecodingHeaderState;

    fn validate_whether_handle_headers_or_continuation_frame(&self, frame: &FrameFacade, ctx: &StreamContext)
    -> anyhow::Result<()>
    {
        match frame {
            FrameFacade::ContinuationFrame(_) => {
                if matches!(ctx.decoding_header_state(), DecodingHeaderState::Initial) ||
                   matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Completed) ||
                   (
                    matches!(ctx.decoding_header_state(), DecodingHeaderState::Inflight) &&
                    matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Initial)
                   ) ||
                   (
                    matches!(ctx.decoding_header_state(), DecodingHeaderState::Completed)  &&
                    matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Initial)
                   ) {
                    //    A CONTINUATION frame MUST be preceded by a HEADERS, PUSH_PROMISE or
                    //    CONTINUATION frame without the END_HEADERS flag set.  A recipient
                    //    that observes violation of this rule MUST respond with a connection
                    //    error (Section 5.4.1) of type PROTOCOL_ERROR.
                    return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
                }
                Ok(())
            },
            FrameFacade::HeadersFrame(_) => {
                if (!matches!(ctx.decoding_header_state(), DecodingHeaderState::Initial) &&
                    !matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Initial)
                   ) ||
                   (
                    matches!(ctx.decoding_header_state(), DecodingHeaderState::Completed)  &&
                    matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Inflight)
                   ) ||
                   (
                    matches!(ctx.decoding_header_state(), DecodingHeaderState::Completed)  &&
                    matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Ready)
                   ) ||
                   (
                    matches!(ctx.decoding_header_state(), DecodingHeaderState::Completed)  &&
                    matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Completed)
                   )
                {
                    return Err(Http2Error::ConnectionError(ErrorCode::ProtocolError).into());
                }
                Ok(())
            }
            _ => Ok(())
        }
    }

    fn maybe_next_state_from_client(self, frame: &FrameFacade, ctx: StreamContext) -> anyhow::Result<Stream>;

    fn maybe_next_state_from_me(self, frame: &FrameFacade, ctx: StreamContext) -> anyhow::Result<Stream>;

}

pub enum Stream {
    IdleStream(IdleStreamInner, StreamContext),
    OpenStream(OpenStreamInner, StreamContext),
    HalfClosedRemoteStream(HalfClosedRemoteStreamInner, StreamContext),
    HalfClosedLocalStream(HalfClosedLocalStreamInner, StreamContext),
    ClosedStream(CloseStreamInner, StreamContext),
    // ReservedStream,
}


impl Stream {

    fn decoding_header_state(&self) -> DecodingHeaderState {
           self.context_as_ref().decoding_header_state()
    }
    fn decoding_trailers_state(&self) -> DecodingHeaderState {
        self.context_as_ref().decoding_trailers_state()
    }

    pub fn init_stream(stream_id: u32, client_window: u64) -> Stream {
        IdleStream(
            IdleStreamInner {},
            StreamContext::with(stream_id, client_window)
        )
    }

    pub fn context_as_ref(&self) -> &StreamContext {
        match self {
            Stream::IdleStream(_, ctx) => ctx,
            Stream::OpenStream(_, ctx) => ctx,
            Stream::HalfClosedRemoteStream(_, ctx) => ctx,
            Stream::HalfClosedLocalStream(_, ctx) => ctx,
            Stream::ClosedStream(_, ctx) => ctx,
        }
    }

    pub fn context_as_mut_ref(&mut self) -> &mut StreamContext {
        match self {
            Stream::IdleStream(_, ctx) => ctx,
            Stream::OpenStream(_, ctx) => ctx,
            Stream::HalfClosedRemoteStream(_, ctx) => ctx,
            Stream::HalfClosedLocalStream(_, ctx) => ctx,
            Stream::ClosedStream(_, ctx) => ctx,
        }
    }

    pub fn validate_whether_handle_headers_or_continuation_frame(&self, frame: &FrameFacade) -> anyhow::Result<()> {
        match self {
            Stream::IdleStream(inner, ctx) => inner.validate_whether_handle_headers_or_continuation_frame(frame, ctx),
            Stream::OpenStream(inner, ctx) => inner.validate_whether_handle_headers_or_continuation_frame(frame, ctx),
            Stream::HalfClosedRemoteStream(inner, ctx) => inner.validate_whether_handle_headers_or_continuation_frame(frame, ctx),
            Stream::HalfClosedLocalStream(inner, ctx) => inner.validate_whether_handle_headers_or_continuation_frame(frame, ctx),
            Stream::ClosedStream(inner, ctx) => inner.validate_whether_handle_headers_or_continuation_frame(frame, ctx),
            _ => Ok(()),
        }
    }

    pub fn maybe_next_state_from_client(self, frame: &FrameFacade) -> anyhow::Result<Stream> {
        match self {
            Stream::IdleStream(inner, ctx) => inner.maybe_next_state_from_client(frame, ctx),
            Stream::OpenStream(inner, ctx) => inner.maybe_next_state_from_client(frame, ctx),
            Stream::HalfClosedRemoteStream(inner, ctx) => inner.maybe_next_state_from_client(frame, ctx),
            Stream::HalfClosedLocalStream(inner, ctx) => inner.maybe_next_state_from_client(frame, ctx),
            Stream::ClosedStream(inner, ctx) => inner.maybe_next_state_from_client(frame, ctx),
        }
    }

    pub fn maybe_next_state_from_me(self, frame: &FrameFacade) -> anyhow::Result<Stream> {
        match self {
            Stream::IdleStream(inner, ctx) => inner.maybe_next_state_from_me(frame, ctx),
            Stream::OpenStream(inner, ctx) => inner.maybe_next_state_from_me(frame, ctx),
            Stream::HalfClosedRemoteStream(inner, ctx) => inner.maybe_next_state_from_me(frame, ctx),
            Stream::HalfClosedLocalStream(inner, ctx) => inner.maybe_next_state_from_me(frame, ctx),
            Stream::ClosedStream(inner, ctx) => inner.maybe_next_state_from_me(frame, ctx),
        }
    }

}



