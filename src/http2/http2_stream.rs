use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::sync::Arc;
use anyhow::{
    anyhow,
    bail,
};
use bytes::{
    Bytes,
    BytesMut
};
use fluke_h2_parse::enumflags2::BitFlags;
use fluke_h2_parse::{
    DataFlags,
    Frame,
    FrameType,
    HeadersFlags,
    StreamId
};
use fluke_hpack::{
    Encoder
};
use tokio::select;
use tokio::sync::mpsc::{
    Sender,
    Receiver
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, info_span, Span, field};
use tracing::field::debug;
use FrameFacade::PriorityFrame;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::{
    ContinuationFrame,
    DataFrame,
    GoawayFrame,
    HeadersFrame,
    PingFrame,
    RstStreamFrame,
    WindowUpdateFrame
};
use crate::http2::frame_data::DataFrameFacade;
use crate::http2::frame_headers::HeadersFrameFacade;
use crate::http2::http2_errors::{
    ErrorCode,
    Http2Error
};
use crate::http2::http2_response::Http2Response;
use crate::http_connection_context::Http2ConnectionContext1;
use crate::http_request_context::{
    Http2RequestContext,
    RequestContextBuilder
};

#[derive(Debug)]
pub enum ReaderChannelMessage {

    // Reader -> Connection
    MaybeFrameParsed { frame: FrameFacade },

    // Reader -> Connection
    ErrorMessage,
}

#[derive(Debug)]
pub enum Http2ChannelMessage {

    // Connection -> Stream
    FrameForRequest { frame: FrameFacade },


    // Connection -> Stream
    ResponseObject { response: Http2Response },

    // Stream -> Connection
    RequestContextForDispatch { req_ctx: Http2RequestContext, stream_id: u32 },

    // Stream -> Connection
    DoResponseToClient { frames: Vec<FrameFacade>, stream_id: u32 },

    // Headers
    DecodingHeadersRequest { payload: Bytes, stream_id: u32 },
    DecodingHeadersResponse { headers: HashMap<String, String>, maybe_trailer: HashSet<String>, error: Option<Http2Error> },

    // Trailers
    DecodingTrailersRequest { payload: Bytes, trailers: HashSet<String>, stream_id: u32 },
    DecodingTrailersResponse { headers: HashMap<String, String>, stream_id: u32 },

    // Stream -> Connection
    Error { error: anyhow::Error },

    // Stream -> Connection
    ActiveStreamCount(ActiveStreamCountCommand, u32),
}

#[derive(Debug)]
pub enum ActiveStreamCountCommand {
    Increment,
    Decrement,
    Keep,
}

#[derive(Debug)]
pub enum DecodingHeaderState {
    Initial,
    KeepGoing,
    Ready,
    WaitingResponseFromDecoder,
    Completed,
}


pub struct Http2Stream {
    stream_id: u32,
    stream_state: StreamState,

    receiver_from_conn: Receiver<Http2ChannelMessage>,
    sender_to_conn: Arc<Sender<Http2ChannelMessage>>,

    window_about_client : u32,
    window_about_me     : u32,

    // Inflight Headers & Body
    inflight_headers: VecDeque<FrameFacade>,
    decoding_header_state: DecodingHeaderState,

    maybe_trailers: Option<HashSet<String>>,
    inflight_trailers_headers: VecDeque<FrameFacade>,
    decoding_trailers_state: DecodingHeaderState,

    composited_header: Option<HashMap<String, String>>,

    inflight_bodies: VecDeque<FrameFacade>,

    // For Graceful shutdown
    cancellation_token: CancellationToken,

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

}

// TODO
// Frame Pipeline을 Parser → Validator → StateMachine → Apply/Emit 식으로 레이어링하면 테스트 용이성↑.

impl Http2Stream {

    fn new(stream_id: u32,
           receiver_from_conn: Receiver<Http2ChannelMessage>,
           sender_to_conn: Arc<Sender<Http2ChannelMessage>>,
           window_size: u32,
           cancellation_token: CancellationToken
    ) -> Self {

        Http2Stream {
            stream_id,
            receiver_from_conn,
            sender_to_conn,
            cancellation_token,

            inflight_headers: VecDeque::new(),
            decoding_header_state: DecodingHeaderState::Initial,
            composited_header: None,

            maybe_trailers: None,
            inflight_trailers_headers: VecDeque::new(),
            decoding_trailers_state: DecodingHeaderState::Initial,

            inflight_bodies: VecDeque::new(),

            stream_state: StreamState::Idle,
            window_about_client: window_size,
            window_about_me: 65535,
            pending_frames: VecDeque::new(),

            // inflight_headers_end: false,
            request_end: false,
            should_terminate_stream: false,

            // http2_request_context: None,
            dispatched: false,
            response_completed: false,

            send_end_stream_completed: false,
            send_end_headers_completed: false,

            error: None,
        }
    }

    pub fn with(stream_id: u32,
                receiver_from_conn: Receiver<Http2ChannelMessage>,
                sender_to_conn: Arc<Sender<Http2ChannelMessage>>,
                window_size: u32,
                cancellation_token: CancellationToken
    ) -> Self {
        Self::new(stream_id,
                  receiver_from_conn,
                  sender_to_conn,
                  window_size,
                  cancellation_token,
        )
    }

    async fn send_frames(&mut self, payload_size: usize) {

    }

    async fn send_request_ctx(&mut self, request_ctx: Http2RequestContext) -> anyhow::Result<()> {
        Ok(
            self.sender_to_conn
                .send(Http2ChannelMessage::RequestContextForDispatch { req_ctx: request_ctx, stream_id: self.stream_id })
                .await?
        )
    }

    fn composite_body(&mut self) -> () {
    }

    async fn maybe_composite_headers(&mut self) -> anyhow::Result<()>
    {
        if !matches!(self.decoding_header_state, DecodingHeaderState::Ready)
        {
            return Ok(());
        }

        self.decoding_header_state = DecodingHeaderState::WaitingResponseFromDecoder;

        let mut header_fragments = BytesMut::new();
        while let Some(mut frame) = self.inflight_headers.pop_front() {
            let bytes = match frame {
                HeadersFrame(mut f) => f.get_header_fragment(),
                ContinuationFrame(mut f) => f.get_header_fragment(),
                _ => Err(anyhow!("unexpected frame type"))
            }?;
            header_fragments.extend_from_slice(&bytes);
        }

        self.sender_to_conn
            .send(
                Http2ChannelMessage::DecodingHeadersRequest { payload: header_fragments.freeze(), stream_id: self.stream_id }
            )
            .await?;

        Ok(())
    }

    async fn maybe_composite_trailers(&mut self) -> anyhow::Result<()>
    {
        if self.request_end &&
           matches!(self.decoding_trailers_state, DecodingHeaderState::Ready) &&
           matches!(self.decoding_header_state, DecodingHeaderState::Completed)
        {
            let mut header_fragments = BytesMut::new();
            while let Some(mut frame) = self.inflight_trailers_headers.pop_front() {
                let bytes = match frame {
                    HeadersFrame(mut f) => f.get_header_fragment(),
                    ContinuationFrame(mut f) => f.get_header_fragment(),
                    _ => Err(anyhow!("unexpected frame type"))
                }?;
                header_fragments.extend_from_slice(&bytes);
            }

            self.decoding_trailers_state = DecodingHeaderState::WaitingResponseFromDecoder;
            self.sender_to_conn
                .send(
                    Http2ChannelMessage::DecodingTrailersRequest {
                        payload: header_fragments.freeze(),
                        trailers: self.maybe_trailers.as_ref().unwrap().clone(),
                        stream_id: self.stream_id
                    }
                )
                .await?;
        }
        Ok(())
    }

    async fn handle_response_object(&mut self, response: Http2Response) -> anyhow::Result<StreamResult>
    {
        debug!("stream got response object {:?}", response);

        let mut response_frames = Vec::new();

        // HeadersFlags::EndHeaders
        let mut flags = BitFlags::<HeadersFlags>::empty();
        flags.insert(HeadersFlags::EndStream);
        flags.insert(HeadersFlags::EndHeaders);
        let f = Frame::new(FrameType::Headers(flags), StreamId(self.stream_id));

        let mut enc = Encoder::default();

        // TODO : 이 부분 개선.
        let (headers, body) = response.into_parts();

        let h: HashMap<Box<[u8]>, Box<[u8]>> = headers.into_iter()
            .map(|(k,v)| (k.into_bytes().into_boxed_slice(), v.into_bytes().into_boxed_slice()))
            .collect();
        let k = enc.encode(h.iter().map(|(k,v)| (k.as_ref(), v.as_ref())));
        let headers_payload = Bytes::from(k);

        response_frames.push(HeadersFrame(HeadersFrameFacade::with(f, headers_payload)));

        let mut data_flags = BitFlags::<DataFlags>::empty();
        data_flags.insert(DataFlags::EndStream);

        let data_f = Frame::new(FrameType::Data(data_flags), StreamId(self.stream_id));
        let data_payload = if body.is_some() {body.unwrap()} else {Bytes::new()};
        let data_payload_size = data_payload.len();

        // TODO: 사이즈에 따라서 놔뒀다가 다음에 보내야 할 수도 있음.
        let data_frame = DataFrame(DataFrameFacade::with(data_f, data_payload));
        if self.can_send_frame(data_payload_size) {
            self.window_about_client -= data_payload_size as u32;
            response_frames.push(data_frame);
        }
        else {
            debug!("stream couldn't send a frame because of window size. client windoe size: {}, payload size: {}",
                self.window_about_client, data_payload_size
            );
            self.pending_frames.push_back(data_frame);
        }

        self.sender_to_conn
            .send(Http2ChannelMessage::DoResponseToClient { frames: response_frames, stream_id: self.stream_id })
            .await?;


        Ok(StreamResult::Nothing)
    }


    fn handle_trailers_in_progress(&self) -> bool {
        if matches!(self.decoding_header_state, DecodingHeaderState::WaitingResponseFromDecoder) ||
           matches!(self.decoding_header_state, DecodingHeaderState::Completed) {
            return true;
        }
        false
    }

    fn handle_frame_for_request(&mut self, frame: FrameFacade) -> anyhow::Result<StreamResult> {
        // Check frame from client.
        self.validate_from_client(&frame)?;

        // Change Stream State
        let (prev, new) = self.maybe_next_state_from_client(&frame).into();
        let active_stream_count_cmd: ActiveStreamCountCommand = (prev, new).into();

        if frame.has_end_stream_flag() {
            self.request_end = true;
        }

        match frame {
            HeadersFrame(_) if self.handle_trailers_in_progress() => {
                if frame.has_end_headers_flag() {
                    self.decoding_trailers_state = DecodingHeaderState::Ready;
                } else {
                    self.decoding_trailers_state = DecodingHeaderState::KeepGoing;
                }
                self.inflight_trailers_headers.push_back(frame);
            },
            HeadersFrame(_) => {
                if frame.has_end_headers_flag() {
                    self.decoding_header_state = DecodingHeaderState::Ready;
                } else {
                    self.decoding_header_state = DecodingHeaderState::KeepGoing;
                }
                self.inflight_headers.push_back(frame)
            }

            ContinuationFrame(_) => {
                if matches!(self.decoding_header_state, DecodingHeaderState::WaitingResponseFromDecoder) ||
                   matches!(self.decoding_header_state, DecodingHeaderState::Completed)
                {
                    if frame.has_end_headers_flag() {
                        self.decoding_trailers_state = DecodingHeaderState::Ready;
                    } else {
                        self.decoding_trailers_state = DecodingHeaderState::KeepGoing;
                    }
                    self.inflight_trailers_headers.push_back(frame);
                } else {
                    if frame.has_end_headers_flag() {
                        self.decoding_header_state = DecodingHeaderState::Ready;
                    } else {
                        self.decoding_header_state = DecodingHeaderState::KeepGoing;
                    }
                    self.inflight_headers.push_back(frame)
                }
            },
            DataFrame(_) => {
                self.inflight_bodies.push_back(frame)
            }
            WindowUpdateFrame(mut frame) => {
                let window_increment = frame.decode_payload_from()?;
                let val = window_increment as u64 + self.window_about_client as u64;

                //    A sender MUST NOT allow a flow-control window to exceed 2^31-1
                //    octets.  If a sender receives a WINDOW_UPDATE that causes a flow-
                //    control window to exceed this maximum, it MUST terminate either the
                //    stream or the connection, as appropriate.  For streams, the sender
                //    sends a RST_STREAM with an error code of FLOW_CONTROL_ERROR; for the
                //    connection, a GOAWAY frame with an error code of FLOW_CONTROL_ERROR
                //    is sent.
                if val > u64::pow(2, 32) - 1 {
                    return Err(
                        Http2Error::stream_error(self.stream_id, ErrorCode::FlowControlError).into()
                    )
                }

                self.window_about_client += window_increment;
            },
            RstStreamFrame(_) => {

            }
            PingFrame(_) => {

            }

            _ => (),
        };

        Ok(StreamResult::StateChange(active_stream_count_cmd))
    }


    async fn maybe_dispatch_if_needed(&mut self) -> anyhow::Result<()>
    {
        if self.request_end &&
           matches!(self.decoding_header_state, DecodingHeaderState::Completed) &&
           matches!(self.decoding_trailers_state, DecodingHeaderState::Completed) &&
           !self.dispatched  &&
           self.composited_header.is_some()
        {
            let mut ctx_builder: RequestContextBuilder<Http2RequestContext> = RequestContextBuilder::new();
            let headers = self.composited_header.take().unwrap();
            ctx_builder.headers(headers);

            // TODO: Use composite_body();
            // TODO : Inflight Body를 모아서 하나의 Body로 만드는 부분 구현 필요.
            if let Some(f) = self.inflight_bodies.pop_front() {
                let body_payload = f.payload();
                ctx_builder.body(body_payload);
            }

            self.dispatched = true;
            let request_ctx = ctx_builder.build()?;

            debug!("stream try to send a request context to connection process. {:?}", request_ctx);
            self.send_request_ctx(request_ctx).await?
        }

        Ok(())
    }

    async fn response_to_client_if_possible(&mut self) -> anyhow::Result<()> {

        // #1. 내가 전송 가능한지 확인하기. (루프 내에서)
        //    - State 관점
        //    - payload size 관점

        if self.pending_frames.is_empty() {
            return Ok(());
        }

        let mut response_frames = Vec::new();
        while let Some(frame) = self.pending_frames.pop_front() {
            let payload_size = match &frame {
                DataFrame(_) => frame.get_payload_size(),
                _ => 0 // '0' means ignore payload size
            };

            let is_data_frame = match &frame {
                DataFrame(_) => true,
                _ => false
            };

            // Wait WINDOW_UPDATE frame.
            if self.window_about_client == 0 {
                self.pending_frames.push_front(frame);
                break;
            }

            // Wait WINDOW_UPDATE frame.
            if (self.window_about_client < payload_size as u32) && (is_data_frame) {
                self.pending_frames.push_front(frame);
                break;
            }

            // Validate.
            if let Err(e) = self.validate_from_me(&frame) {
                self.pending_frames.push_front(frame);
                bail!("Unexpected State");
            }

            // State Change
            self.maybe_next_state_from_me(&frame);

            if frame.has_end_stream_flag() {
                self.send_end_stream_completed = true;
            }

            if frame.has_end_headers_flag() {
                self.send_end_headers_completed = true;
            }

            response_frames.push(frame);
        }

        if !response_frames.is_empty() {
            self.sender_to_conn
                .send(Http2ChannelMessage::DoResponseToClient { frames: response_frames, stream_id: self.stream_id })
                .await?;

            if (self.dispatched &&
                self.send_end_stream_completed &&
                self.send_end_headers_completed) {
                self.response_completed = true;
            }
        }

        Ok(())
    }


    // TODO: 1. tokio token으로 graceful shutdown.
    // TODO: 2. RequestCtx 만드는 부분, Frame을 받는 부분을 굳이 나눌 필요가 있는지.
    // TODO: 3. Inflight Header Frame -> 조립
    pub async fn serve(mut self) -> anyhow::Result<()> {
        info!("the stream starts to serve.");
        loop {
            if self.should_terminate_stream {
                break;
            }

            self.maybe_composite_headers().await?;
            self.maybe_composite_trailers().await?;
            self.maybe_dispatch_if_needed().await?;
            self.response_to_client_if_possible().await?;

            let result = select! {
                // respect cancel first.
                biased;

                // Task #1 : Graceful shutdown
                _ = self.cancellation_token.cancelled() => {
                    self.should_terminate_stream = true;
                    Ok(StreamResult::Nothing)
                }

                // Task #2 : From Connection
                Some(msg) = self.receiver_from_conn.recv() => {
                    debug!("Received Messages : {:?}", msg);
                    match msg {
                        // #1 : Message for Request.
                        Http2ChannelMessage::FrameForRequest { frame } => self.handle_frame_for_request(frame),

                        // #2 : Message for Response.
                        Http2ChannelMessage::ResponseObject { response } => self.handle_response_object(response).await,

                        // #3 : Message from header composite
                        Http2ChannelMessage::DecodingHeadersResponse { headers, maybe_trailer, error } => {
                            // It has potential Race Condition.
                            // 1. stream -> connection : Ask to decoding header frame
                            // 2. connection -> stream : Header Frame -> Error Occur.
                            // 3. stream -> connection : Response decoding header with trailers.
                            // in this case, http2 don't expect protocol error at step 2.
                            self.decoding_header_state = DecodingHeaderState::Completed;
                            let is_trailer_empty = maybe_trailer.is_empty();

                            if !is_trailer_empty {
                                self.maybe_trailers = Some(maybe_trailer);
                            } else {
                                self.decoding_trailers_state = DecodingHeaderState::Completed;
                            }

                            if is_trailer_empty && !self.inflight_trailers_headers.is_empty() {
                                Err(Http2Error::ConnectionError(ErrorCode::ProtocolError).into())
                            } else {
                                self.composited_header.replace(headers);
                                Ok(StreamResult::Nothing)
                            }
                        }

                        Http2ChannelMessage::DecodingTrailersResponse { headers, stream_id } => {
                            self.composited_header.as_mut()
                                .unwrap()
                                .extend(headers);
                            self.decoding_trailers_state = DecodingHeaderState::Completed;
                            Ok(StreamResult::Nothing)
                        }


                        // #3 : Unexpected Message.
                        _ => Ok(StreamResult::Nothing)
                    }
                }
            };

            match result {
                Ok(StreamResult::StateChange(active_stream_count_cmd)) => {
                    self.sender_to_conn
                        .send(
                            Http2ChannelMessage::ActiveStreamCount(active_stream_count_cmd, self.stream_id)
                        )
                        .await?
                },
                Err(error) => {
                    self.sender_to_conn
                        .send(
                            Http2ChannelMessage::Error { error }
                        )
                        .await?;
                }
                _ => ()
            }
        }

        debug!("Shutting down stream.");
        Ok(())
    }


    fn flush(&mut self) -> Vec<FrameFacade> {
        let mut frames_for_flush = Vec::new();

        while let Some(frame_facade) = self.pending_frames.pop_front() {
            let payload_size = match frame_facade {
                FrameFacade::DataFrame(_) => frame_facade.get_payload_size(),
                _                         => 0 as usize // Don't care if frame is not data frame.
            };
            if self.can_send_frame(payload_size) {
                self.window_about_client -= payload_size as u32;
                frames_for_flush.push(frame_facade);
            } else {
                self.pending_frames.push_front(frame_facade);
                break;
            };
        };

        frames_for_flush
    }

    fn can_send_frame(&mut self, payload_size: usize) -> bool {
        if self.window_about_client == 0 {
            return false;
        }

        self.window_about_client >= payload_size as u32
    }

    fn validate_from_client(&mut self, frame: &FrameFacade) -> anyhow::Result<()>{
        if matches!(frame, FrameFacade::ContinuationFrame(_)) &&
        (
            matches!(self.decoding_header_state, DecodingHeaderState::Initial) ||
            matches!(self.decoding_trailers_state, DecodingHeaderState::Completed) ||
            (
                matches!(self.decoding_header_state, DecodingHeaderState::WaitingResponseFromDecoder) &&
                matches!(self.decoding_trailers_state, DecodingHeaderState::Initial)
            ) ||
            (
                matches!(self.decoding_header_state, DecodingHeaderState::Completed)  &&
                matches!(self.decoding_trailers_state, DecodingHeaderState::Initial)
             )
        )
        {
            //    A CONTINUATION frame MUST be preceded by a HEADERS, PUSH_PROMISE or
            //    CONTINUATION frame without the END_HEADERS flag set.  A recipient
            //    that observes violation of this rule MUST respond with a connection
            //    error (Section 5.4.1) of type PROTOCOL_ERROR.
            return Err(Http2Error::connection_error(ErrorCode::ProtocolError).into());
        }

        if matches!(frame, FrameFacade::HeadersFrame(_)) && (
            (
                !matches!(self.decoding_header_state, DecodingHeaderState::Initial) &&
                !matches!(self.decoding_trailers_state, DecodingHeaderState::Initial)
            ) ||
            (
                matches!(self.decoding_header_state, DecodingHeaderState::Completed)  &&
                matches!(self.decoding_trailers_state, DecodingHeaderState::WaitingResponseFromDecoder)
            ) ||
            (
                matches!(self.decoding_header_state, DecodingHeaderState::Completed)  &&
                matches!(self.decoding_trailers_state, DecodingHeaderState::Ready)
            ) ||
            (
                matches!(self.decoding_header_state, DecodingHeaderState::Completed)  &&
                matches!(self.decoding_trailers_state, DecodingHeaderState::Completed)
            )
        )
        {
            return Err(Http2Error::ConnectionError(ErrorCode::ProtocolError).into());
        }

        match (self.stream_state, frame) {
            (StreamState::Idle, HeadersFrame(_)) => Ok(()),
            (StreamState::Idle, PriorityFrame(_)) => Ok(()),

            // Open -> Data frame, Priority Frame, Continuation Frame.
            (StreamState::Open, DataFrame(_)) => Ok(()),
            (StreamState::Open, PriorityFrame(_)) => Ok(()),
            (StreamState::Open, ContinuationFrame(_)) => Ok(()),

            // HalfClosedRemote -> Priority Frame, Continuation Frame
            (StreamState::HalfClosedRemote, PriorityFrame(_)) => Ok(()),
            (StreamState::HalfClosedRemote, ContinuationFrame(_)) => Ok(()),
            (StreamState::HalfClosedRemote, WindowUpdateFrame(_)) => Ok(()),

            // HalfClosedLocal, // Allow Frame: Data Frame (from client), Priority Frame, Continuation Frame.
            (StreamState::HalfClosedLocal, DataFrame(_)) => Ok(()),
            (StreamState::HalfClosedLocal, PriorityFrame(_)) => Ok(()),
            (StreamState::HalfClosedLocal, ContinuationFrame(_)) => Ok(()),

            // **** Validate Fail.
            //    RST_STREAM frames MUST NOT be sent for a stream in the "idle" state.
            //    If a RST_STREAM frame identifying an idle stream is received, the
            //    recipient MUST treat this as a connection error (Section 5.4.1) of
            //    type PROTOCOL_ERROR.  (https://datatracker.ietf.org/doc/html/rfc7540#section-6.4)
            (StreamState::Idle, RstStreamFrame(_)) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),

            // *** IDLE
            //       Receiving any frame other than HEADERS or PRIORITY on a stream in
            //       this state MUST be treated as a connection error (Section 5.4.1)
            //       of type PROTOCOL_ERROR.
            (StreamState::Idle, DataFrame(_)) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            (StreamState::Idle, PingFrame(_)) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            (StreamState::Idle, ContinuationFrame(_)) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            (StreamState::Idle, GoawayFrame(_)) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),
            (StreamState::Idle, WindowUpdateFrame(_)) => Err(Http2Error::connection_error(ErrorCode::ProtocolError).into()),

            // *** Half-closed (Remote)
            //    If an endpoint receives additional frames, other than
            //    WINDOW_UPDATE, PRIORITY, or RST_STREAM, for a stream that is in
            //    this state, it MUST respond with a stream error (Section 5.4.2) of
            //    type STREAM_CLOSED.
            (StreamState::HalfClosedRemote, DataFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),
            (StreamState::HalfClosedRemote, HeadersFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),
            (StreamState::HalfClosedRemote, PingFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),
            // (StreamState::HalfClosedRemote, ContinuationFrame(_)) => Err(
            //     Http2Error::StreamError { stream_id: self.stream_id, code: ErrorCode::StreamClosed}.into()
            // ),
            (StreamState::HalfClosedRemote, GoawayFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),


            // *** Closed
            // An endpoint MUST NOT send frames other than PRIORITY on a closed
            // stream.  An endpoint that receives any frame other than PRIORITY
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
            (StreamState::Closed, PriorityFrame(_)) => Ok(()),

            (StreamState::Closed, DataFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),
            (StreamState::Closed, HeadersFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),
            (StreamState::Closed, PingFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),
            (StreamState::Closed, GoawayFrame(_)) => Err(
                Http2Error::stream_error(self.stream_id, ErrorCode::StreamClosed).into()
            ),

            // Should ignore.
            // (StreamState::Closed, RstStreamFrame(_)) => Err(
            //     Http2Error::StreamError { stream_id: self.stream_id, code: ErrorCode::StreamClosed}.into()
            // ),
            // (StreamState::Closed, WindowUpdateFrame(_)) => Err(
            //     Http2Error::StreamError { stream_id: self.stream_id, code: ErrorCode::StreamClosed}.into()
            // ),

            _ => Ok(())
        }

    }

    fn validate_from_me(&mut self, frame: &FrameFacade) -> anyhow::Result<()>{

        match self.stream_state {
            StreamState::Idle => {
                match frame {
                    HeadersFrame(_) => Ok(()),
                    PriorityFrame(_) => Ok(()),
                    _ => Err(anyhow!("PROTOCOL_ERROR"))
                }
            },
            _ => Ok(())
        }
    }

    fn maybe_next_state_from_client(&mut self, frame: &FrameFacade) -> (StreamState, StreamState)
    {
        let prev_state = self.stream_state;
        match (self.stream_state, frame) {
            (StreamState::Idle, HeadersFrame(_)) if frame.has_end_stream_flag()                     => self.stream_state = StreamState::HalfClosedRemote,
            (StreamState::Idle, HeadersFrame(_))                                                    => self.stream_state = StreamState::Open,

            (StreamState::Open, HeadersFrame(_)) if frame.has_end_stream_flag()                     => self.stream_state = StreamState::HalfClosedRemote,
            (StreamState::Open, DataFrame(_)) if frame.has_end_stream_flag()                        => self.stream_state = StreamState::HalfClosedRemote,



            (StreamState::Open, RstStreamFrame(_))                                                  => self.stream_state = StreamState::Closed,

            (StreamState::HalfClosedLocal, HeadersFrame(_)) if frame.has_end_stream_flag()          => self.stream_state = StreamState::Closed,
            (StreamState::HalfClosedLocal, DataFrame(_)) if frame.has_end_stream_flag()             => self.stream_state = StreamState::Closed,
            (StreamState::HalfClosedLocal, RstStreamFrame(_))                                       => self.stream_state = StreamState::Closed,

            (StreamState::HalfClosedRemote, RstStreamFrame(_))                                      => self.stream_state = StreamState::Closed,

            // Don't change state. just for explicit.
            (StreamState::HalfClosedRemote, ContinuationFrame(_))                                   => self.stream_state = StreamState::HalfClosedRemote,

            _ => ()
        };

        let new_state = self.stream_state;
        Span::current().record("stream_state", &field::debug(new_state));
        debug!("State Update.");
        (prev_state, new_state)
    }

    fn maybe_next_state_from_me(&mut self, frame: &FrameFacade) {


        match (self.stream_state, frame) {
            (StreamState::Idle, HeadersFrame(_)) if frame.has_end_stream_flag()                     => self.stream_state = StreamState::HalfClosedLocal,
            (StreamState::Idle, HeadersFrame(_))                                                    => self.stream_state = StreamState::Open,

            (StreamState::Open, HeadersFrame(_)) if frame.has_end_stream_flag()                     => self.stream_state = StreamState::HalfClosedLocal,
            (StreamState::Open, DataFrame(_)) if frame.has_end_stream_flag()                        => self.stream_state = StreamState::HalfClosedLocal,

            (StreamState::Open, RstStreamFrame(_))                                                  => self.stream_state = StreamState::Closed,

            (StreamState::HalfClosedLocal, RstStreamFrame(_))                                       => self.stream_state = StreamState::Closed,

            (StreamState::HalfClosedRemote, HeadersFrame(_)) if frame.has_end_stream_flag()         => self.stream_state = StreamState::Closed,
            (StreamState::HalfClosedRemote, DataFrame(_)) if frame.has_end_stream_flag()            => self.stream_state = StreamState::Closed,
            (StreamState::HalfClosedRemote, RstStreamFrame(_))                                      => self.stream_state = StreamState::Closed,
            _                                                                                       => ()
        }
    }

}



// https://datatracker.ietf.org/doc/html/rfc7540#section-5.1

#[derive(Debug, Copy, Clone)]
enum StreamState {
    // The lifecycle of a stream is shown in Figure 2.
    //
    //                                 +--------+
    //                         send PP |        | recv PP
    //                        ,--------|  idle  |--------.
    //                       /         |        |         \
    //                      v          +--------+          v
    //               +----------+          |           +----------+
    //               |          |          | send H /  |          |
    //        ,------| reserved |          | recv H    | reserved |------.
    //        |      | (local)  |          |           | (remote) |      |
    //        |      +----------+          v           +----------+      |
    //        |          |             +--------+             |          |
    //        |          |     recv ES |        | send ES     |          |
    //        |   send H |     ,-------|  open  |-------.     | recv H   |
    //        |          |    /        |        |        \    |          |
    //        |          v   v         +--------+         v   v          |
    //        |      +----------+          |           +----------+      |
    //        |      |   half   |          |           |   half   |      |
    //        |      |  closed  |          | send R /  |  closed  |      |
    //        |      | (remote) |          | recv R    | (local)  |      |
    //        |      +----------+          |           +----------+      |
    //        |           |                |                 |           |
    //        |           | send ES /      |       recv ES / |           |
    //        |           | send R /       v        send R / |           |
    //        |           | recv R     +--------+   recv R   |           |
    //        | send R /  `----------->|        |<-----------'  send R / |
    //        | recv R                 | closed |               recv R   |
    //        `----------------------->|        |<----------------------'
    //                                 +--------+
    //           send:   endpoint sends this frame
    //           recv:   endpoint receives this frame
    //
    //           H:  HEADERS frame (with implied CONTINUATIONs)
    //           PP: PUSH_PROMISE frame (with implied CONTINUATIONs)
    //           ES: END_STREAM flag
    //           R:  RST_STREAM frame

    //       idle:
    //       All streams start in the "idle" state.
    //
    //       The following transitions are valid from this state:
    //
    //       *  Sending or receiving a HEADERS frame causes the stream to
    //          become "open".  The stream identifier is selected as described
    //          in Section 5.1.1.  The same HEADERS frame can also cause a
    //          stream to immediately become "half-closed".
    //
    //       *  Sending a PUSH_PROMISE frame on another stream reserves the
    //          idle stream that is identified for later use.  The stream state
    //          for the reserved stream transitions to "reserved (local)".
    //
    //       *  Receiving a PUSH_PROMISE frame on another stream reserves an
    //          idle stream that is identified for later use.  The stream state
    //          for the reserved stream transitions to "reserved (remote)".
    //
    //       *  Note that the PUSH_PROMISE frame is not sent on the idle stream
    //          but references the newly reserved stream in the Promised Stream
    //          ID field.
    //
    //       Receiving any frame other than HEADERS or PRIORITY on a stream in
    //       this state MUST be treated as a connection error (Section 5.4.1)
    //       of type PROTOCOL_ERROR.
    Idle, // Allow Frame: Headers, Priority. other are prohibit.

    Reserved, // Allow Frame: Priority Frame

    //     open:
    //       A stream in the "open" state may be used by both peers to send
    //       frames of any type.  In this state, sending peers observe
    //       advertised stream-level flow-control limits (Section 5.2).
    //
    //       From this state, either endpoint can send a frame with an
    //       END_STREAM flag set, which causes the stream to transition into
    //       one of the "half-closed" states.  An endpoint sending an
    //       END_STREAM flag causes the stream state to become "half-closed
    //       (local)"; an endpoint receiving an END_STREAM flag causes the
    //       stream state to become "half-closed (remote)".
    //
    //       Either endpoint can send a RST_STREAM frame from this state,
    //       causing it to transition immediately to "closed".
    Open, // Allow Frame: Data frame, Priority Frame, Continuation Frame.

    HalfClosedLocal, // Allow Frame: Data Frame (from client), Priority Frame, Continuation Frame.
    // #1 send open : Can I send? => After Server send END_STREAM. send_open is 'false'.
    // #2 recv open : Can I receive? => After Client send END_STREAM, recv_open is 'false'.
    //    half-closed (local):
    //       A stream that is in the "half-closed (local)" state cannot be used
    //       for sending frames other than WINDOW_UPDATE, PRIORITY, and
    //       RST_STREAM.
    //
    //       A stream transitions from this state to "closed" when a frame that
    //       contains an END_STREAM flag is received or when either peer sends
    //       a RST_STREAM frame.
    //
    //       An endpoint can receive any type of frame in this state.
    //       Providing flow-control credit using WINDOW_UPDATE frames is
    //       necessary to continue receiving flow-controlled frames.  In this
    //       state, a receiver can ignore WINDOW_UPDATE frames, which might
    //       arrive for a short period after a frame bearing the END_STREAM
    //       flag is sent.
    //
    //       PRIORITY frames received in this state are used to reprioritize
    //       streams that depend on the identified stream.

    HalfClosedRemote, // Allow Frame : Priority Frame, Continuation Frame
    //    half-closed (remote):
    //       A stream that is "half-closed (remote)" is no longer being used by
    //       the peer to send frames.  In this state, an endpoint is no longer
    //       obligated to maintain a receiver flow-control window.
    //
    //       If an endpoint receives additional frames, other than
    //       WINDOW_UPDATE, PRIORITY, or RST_STREAM, for a stream that is in
    //       this state, it MUST respond with a stream error (Section 5.4.2) of
    //       type STREAM_CLOSED.
    //
    //       A stream that is "half-closed (remote)" can be used by the
    //       endpoint to send frames of any type.  In this state, the endpoint
    //       continues to observe advertised stream-level flow-control limits
    //       (Section 5.2).
    //
    //       A stream can transition from this state to "closed" by sending a
    //       frame that contains an END_STREAM flag or when either peer sends a
    //       RST_STREAM frame.
    Closed // Allow Frame : Priority Frame
}


pub struct Http2StreamHandler {}

impl Http2StreamHandler {

    pub fn should_init_stream(frame: &FrameFacade, conn_context: &Http2ConnectionContext1) -> bool {
        let sid = frame.stream_id();
        if sid == 0 {
            return false;
        };

        if conn_context.is_exist(sid) {
            return false;
        }

        debug!("new stream {} is created.", sid);
        true
    }



    pub fn should_ignore(frame: &FrameFacade, conn_context: &Http2ConnectionContext1) -> bool{
        let sid = frame.stream_id();
        if let Some(last_seen_max_id) = conn_context.get_go_away_stream_id() {
            return sid > last_seen_max_id;
        }

        false
    }
}


impl From<(StreamState, StreamState)> for ActiveStreamCountCommand {
    fn from((prev, new): (StreamState, StreamState)) -> Self {
        match (prev, new) {
            (StreamState::Idle, StreamState::Open) => ActiveStreamCountCommand::Increment,
            (StreamState::Idle, StreamState::HalfClosedRemote) => ActiveStreamCountCommand::Increment,
            (StreamState::Idle, StreamState::HalfClosedLocal) => ActiveStreamCountCommand::Increment,

            (StreamState::Open, StreamState::Closed) => ActiveStreamCountCommand::Decrement,
            (StreamState::HalfClosedRemote, StreamState::Closed) => ActiveStreamCountCommand::Decrement,
            (StreamState::HalfClosedLocal, StreamState::Closed) => ActiveStreamCountCommand::Decrement,

            _ => ActiveStreamCountCommand::Keep,
        }
    }
}


pub enum StreamResult {
    StateChange(ActiveStreamCountCommand),
    Nothing,
    DecodingTrailers,
    Error(anyhow::Error),
}


pub fn new_stream_span(sid: u32, init_window_size: u32) -> Span {
    info_span!(
        "stream",
        stream_id=sid,
        // 1) 생성 시점에 Debug/Display 래퍼로 넣기 (가장 간단)
        // ? = Debug, % = Display
        stream_state=?StreamState::Idle,
        stream_window_size=init_window_size
    )
}