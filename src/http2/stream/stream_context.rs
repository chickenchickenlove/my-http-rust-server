use std::collections::{HashMap, HashSet, VecDeque};
use tokio_util::sync::CancellationToken;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::http2_errors::Http2Error;
use crate::http2::http2_stream::DecodingHeaderState;

pub struct StreamContext {
    stream_id: u32,
    decoding_header_state: DecodingHeaderState,
    decoding_trailers_state: DecodingHeaderState,

    // Inflight Headers & Body
    inflight_headers: VecDeque<FrameFacade>,

    maybe_trailers: Option<HashSet<String>>,
    inflight_trailers_headers: VecDeque<FrameFacade>,

    composited_header: Option<HashMap<String, String>>,

    inflight_bodies: VecDeque<FrameFacade>,

    // For Graceful shutdown
    // cancellation_token: CancellationToken,

    // Because of window size,
    pending_frames: VecDeque<FrameFacade>,

    // Flags
    // inflight_headers_end: bool,
    request_end: bool,

    should_terminate_stream: bool,

    dispatched: bool,

    // http2_request_context: Option<Http2RequestContext>,
    response_completed: bool,

    // Stream -> Client
    send_end_headers_completed: bool,
    send_end_stream_completed: bool,

    error: Option<Http2Error>,

    // window
    client_window: u64,
}

impl StreamContext {

    pub fn with(stream_id: u32, client_window: u64) -> Self {
        StreamContext {
            stream_id,
            decoding_header_state: DecodingHeaderState::Initial,
            decoding_trailers_state: DecodingHeaderState::Initial,

            // Inflight Headers & Body
            inflight_headers: VecDeque::new(),

            maybe_trailers: None,
            inflight_trailers_headers: VecDeque::new(),

            composited_header: None,

            inflight_bodies: VecDeque::new(),

            // Because of window size,
            pending_frames: VecDeque::new(),

            // Flags
            // inflight_headers_end: bool,
            request_end: false,

            should_terminate_stream: false,

            dispatched: false,

            // http2_request_context: Option<Http2RequestContext>,
            response_completed: false,

            // Stream -> Client
            send_end_headers_completed: false,
            send_end_stream_completed: false,

            error: None,

            // window
            client_window,

        }
    }

    pub fn handle_trailers_in_progress(&self) -> bool {
        if matches!(self.decoding_header_state, DecodingHeaderState::Inflight) ||
           matches!(self.decoding_header_state, DecodingHeaderState::Completed) {
            return true;
        }
        false
    }

    pub fn update_decoding_trailers_state(&mut self, state: DecodingHeaderState) {
        self.decoding_trailers_state = state
    }

    pub fn update_decoding_headers_state(&mut self, state: DecodingHeaderState) {
        self.decoding_header_state = state
    }

    pub fn add_inflight_trailers_headers(&mut self, frame: FrameFacade) {
        self.inflight_trailers_headers.push_back(frame);
    }

    pub fn add_inflight_headers(&mut self, frame: FrameFacade) {
        self.inflight_headers.push_back(frame)
    }

    pub fn decoding_header_state(&self) -> DecodingHeaderState {
        self.decoding_header_state.clone()
    }

    pub fn decoding_trailers_state(&self) -> DecodingHeaderState {
        self.decoding_trailers_state.clone()
    }

    pub fn add_inflight_body(&mut self, frame: FrameFacade) {
        self.inflight_bodies.push_back(frame);
    }

    pub fn valid_window_size(&mut self, window_size: u64)
        -> bool
    {
        //    A sender MUST NOT allow a flow-control window to exceed 2^31-1
        //    octets.  If a sender receives a WINDOW_UPDATE that causes a flow-
        //    control window to exceed this maximum, it MUST terminate either the
        //    stream or the connection, as appropriate.  For streams, the sender
        //    sends a RST_STREAM with an error code of FLOW_CONTROL_ERROR; for the
        //    connection, a GOAWAY frame with an error code of FLOW_CONTROL_ERROR
        //    is sent.
        let val = window_size + self.client_window;
        val > u64::pow(2, 31) - 1
    }

    pub fn increment_window(&mut self, window_size: u64) {
        self.client_window += window_size;
    }

    pub fn stream_id(&self) -> u32 {
        self.stream_id
    }

    pub fn pop_inflight_header_frame(&mut self) -> Option<FrameFacade> {
        self.inflight_headers.pop_front()
    }

    pub fn pop_inflight_trailer_frame(&mut self) -> Option<FrameFacade> {
        self.inflight_trailers_headers.pop_front()
    }

    pub fn is_request_end(&self) -> bool {
        self.request_end
    }

    pub fn trailers(&mut self) -> Option<HashSet<String>> {
        let previous = self.maybe_trailers.take();
        self.maybe_trailers = None;
        previous
    }

    pub fn is_dispatched(&self) -> bool {
        self.dispatched
    }

    pub fn dispatched(&mut self) {
        self.dispatched = true;
    }

    pub fn has_composite_header(&self) -> bool {
        self.composited_header.is_some()
    }

    pub fn take_composite_header(&mut self) -> HashMap<String, String> {
        let header = self.composited_header.take().unwrap();
        self.composited_header = None;
        header
    }

    pub fn pop_inflight_body(&mut self) -> Option<FrameFacade> {
        self.inflight_bodies.pop_front()
    }


}