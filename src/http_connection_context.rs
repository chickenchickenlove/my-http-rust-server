use std::cmp::max;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use anyhow::{anyhow, bail};
use bytes::Bytes;
use tokio::sync::mpsc::{
    Sender
};
use tokio_util::sync::CancellationToken;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::http2_conn_options::{
    Http2ConnectionOptions,
    PartialHttp2ConnectionOptions
};
use crate::http2_stream::{
    Http2ChannelMessage,
    Http2Stream
};
use crate::http_type::HttpProtocol;


pub trait ConnectionContext {
    const PROTOCOL: HttpProtocol;

    fn should_close(&self) -> bool;
}

pub struct Http1ConnectionContext1 {

}

pub struct Http11ConnectionContext1 {

}


impl Http1ConnectionContext1 {
    pub fn new() -> Self {
        Http1ConnectionContext1 { }
    }
}

impl Http11ConnectionContext1 {

    pub fn new() -> Self {
        Http11ConnectionContext1 {}
    }

}


pub struct Http2ConnectionContext1 {
    streams: HashMap<u32, Http2Stream>,
    streams_channels: HashMap<u32, Sender<Http2ChannelMessage>>,

    //
    http2_options_from_client: Http2ConnectionOptions,

    //
    http2_options_from_me: Http2ConnectionOptions,

    // Channel
    sender_for_stream: Option<Arc<Sender<Http2ChannelMessage>>>,
    sender_for_reader: Option<Arc<Sender<Http2ChannelMessage>>>,

    // State
    inflight_headers_frame_stream_id: Option<u32>,

    current_window_size_of_client: usize,
    current_window_size_of_server: usize,

    //    Streams that are in the "open" state or in either of the "half-
    //    closed" states count toward the maximum number of streams that an
    //    endpoint is permitted to open.  Streams in any of these three states
    //    count toward the limit advertised in the
    //    SETTINGS_MAX_CONCURRENT_STREAMS setting.  Streams in either of the
    //    "reserved" states do not count toward the stream limit.
    current_active_stream_count: u32,
    active_max_stream_id: u32,

    seen_max_stream_id: u32,
    go_away_stream_id: Option<u32>,

    // To cancel stream.
    root_token: CancellationToken,
    reader_token: CancellationToken,
    stream_root_token: CancellationToken,
    child_tokens: HashMap<u32, CancellationToken>,

    // error
    error: Option<anyhow::Error>
}

impl Http2ConnectionContext1 {

    pub fn new() -> Self {

        let cancellation_token = CancellationToken::new();
        let reader_token = cancellation_token.clone();
        let entire_child_token = cancellation_token.child_token();
        Http2ConnectionContext1 {
            streams: HashMap::new(),
            streams_channels: HashMap::new(),
            http2_options_from_client: Http2ConnectionOptions::default(),
            http2_options_from_me: Http2ConnectionOptions::default(),

            current_window_size_of_client: 65535,
            current_window_size_of_server: 65535,
            current_active_stream_count: 0,

            sender_for_stream: None,
            sender_for_reader: None,
            inflight_headers_frame_stream_id: None,

            seen_max_stream_id: 0,
            go_away_stream_id: None,

            // To cancel token
            root_token: cancellation_token,
            reader_token: reader_token,
            stream_root_token: entire_child_token,
            child_tokens: HashMap::new(),

            error: None,
            active_max_stream_id: 0,
        }
    }


    // For The check
    pub fn is_valid_considering_inflight_headers(&self, frame: &FrameFacade) -> bool {
        println!("### is_valid_considering_inflight_headers {:?}", frame);



        if self.inflight_headers_frame_stream_id.is_none() {
            return true;
        }

        if self.inflight_headers_frame_stream_id.unwrap() != frame.stream_id() {
            return false;
        }

        if !matches!(frame, FrameFacade::ContinuationFrame(_)) {
            return false;
        }

        true
    }

    pub fn update_inflight_headers(&mut self, stream_id: u32) {
        self.inflight_headers_frame_stream_id = Some(stream_id)
    }

    pub fn clear_inflight_headers(&mut self) {
        self.inflight_headers_frame_stream_id = None
    }

    pub fn equal_inflight_headers_stream_id(&self, stream_id: u32) -> bool {
        if let Some(v) = self.inflight_headers_frame_stream_id {
            return v == stream_id
        }
        false
    }

    pub fn has_inflight_headers(&self) -> bool {
        self.inflight_headers_frame_stream_id.is_some()
    }


    // When Server try to send a frame to client,
    pub fn get_max_frame_size_from_client(&self) -> usize {
        self.http2_options_from_client.get_max_frame_size() as usize
    }

    pub fn get_max_frame_size_from_me(&self) -> usize {
        self.http2_options_from_me.get_max_frame_size() as usize
    }

    pub fn get_max_concurrent_streams_from_me(&self) -> u32 {
        self.http2_options_from_me.get_max_concurrent_streams()
    }

    pub fn get_max_active_stream_id(&self) -> u32 {
        self.active_max_stream_id
    }

    pub fn update_max_active_stream_id(&mut self, stream_id: u32) {
        self.active_max_stream_id = max(stream_id, self.active_max_stream_id);
    }


    pub fn get_active_streams_count(&self) -> u32 {
        self.current_active_stream_count
    }

    // ************** Related With Token *********************
    pub fn should_close(&self) -> bool {
        self.root_token.is_cancelled()
    }

    pub fn cancel_stream_token(&mut self, stream_id: u32)  {
        if let Some(token) = self.child_tokens.remove(&stream_id) {
            self.current_active_stream_count -= 1;
            token.cancel();
        }
    }

    pub fn get_cancellation_token_by_stream_id(&self, stream_id: u32) -> Option<&CancellationToken> {
        self.child_tokens.get(&stream_id)
    }

    pub fn get_root_token(&self) -> &CancellationToken {
        &self.root_token
    }

    pub fn cancel_root_token(&self) {
        self.root_token.cancel();
    }

    pub fn get_reader_token(&self) -> CancellationToken {
        self.reader_token.clone()
    }

    pub fn cancel_reader_token(&self) {
        self.reader_token.cancel()
    }

    pub fn get_stream_root_token(&self) -> &CancellationToken {
        &self.stream_root_token
    }

    pub fn cancel_stream_root_token(&self) {
        self.stream_root_token.cancel()
    }

    pub fn get_stream_token_by_stream_id(&mut self, stream_id: u32) -> CancellationToken {
        let token = if let Some(t) = self.child_tokens.get(&stream_id) {
            t.clone()
        } else {
            let t = self.stream_root_token.child_token();
            self.child_tokens.insert(stream_id, t.clone());
            t
        };

        token
    }

    // ************** Related With Token *********************

    // When Server receive frames from client,
    pub fn get_go_away_stream_id(&self) -> Option<u32> {
        self.go_away_stream_id
    }

    pub fn update_seen_max_stream_id(&mut self, stream_id: u32) -> () {
        self.seen_max_stream_id = max(self.seen_max_stream_id, stream_id);
    }

    pub fn get_seen_max_stream_id(&mut self) -> u32 {
        self.seen_max_stream_id
    }

    pub fn set_go_away_stream_id(&mut self, goaway_stream_id: u32){
        self.go_away_stream_id = Some(goaway_stream_id)
    }

    pub fn add_stream(&mut self, sid: u32, channel: Sender<Http2ChannelMessage>) {
        self.streams_channels.insert(sid, channel);
        self.child_tokens.insert(sid, self.stream_root_token.child_token());
        self.current_active_stream_count += 1;
    }

    pub fn get_sender_to_channel(&self, sid: u32) -> Option<&Sender<Http2ChannelMessage>> {
        self.streams_channels.get(&sid)
    }

    pub fn is_exist(&self, sid: u32) -> bool {
        self.streams_channels.contains_key(&sid)
    }

    pub fn get_sender(&self) -> anyhow::Result<Arc<Sender<Http2ChannelMessage>>> {
        if let Some(v) = &self.sender_for_stream {
            Ok(v.clone())
        }
        else{
            bail!("Sender is not intialized yet.")
        }
    }

    pub fn set_sender(&mut self, sender: Sender<Http2ChannelMessage>) {
        self.sender_for_stream = Some(Arc::new(sender));
    }

    pub fn set_sender_for_reader(&mut self, sender: Sender<Http2ChannelMessage>) {
        self.sender_for_reader = Some(Arc::new(sender));
    }

    pub fn set_window_size_of_client(&mut self, window_size: u32) {
        self.current_window_size_of_client = window_size as usize;
    }

    pub fn decrement_window_size_of_server(&mut self, window_size: usize) {
        self.current_window_size_of_server = self.current_window_size_of_server - window_size;
    }

    pub fn increment_active_stream_count(&mut self, stream_id: u32) {
        self.current_active_stream_count += 1;
    }

    pub fn decrement_active_stream_count(&mut self) {
        self.current_active_stream_count -= 1;
    }

    pub fn increment_window_size(&mut self, window_size: u32) {
        self.current_window_size_of_client += window_size as usize;
    }


    pub fn decrement_window_size(&mut self, window_size: usize) {
        self.current_window_size_of_client -= window_size;
    }

    pub fn current_window_size(&mut self) -> usize {
        self.current_window_size_of_client.clone()
    }

    pub fn update_options_from_client(&mut self, options: PartialHttp2ConnectionOptions) {
        self.http2_options_from_client.update(options);
    }

    pub fn print_options(&mut self) {
        println!("{:?}", self.http2_options_from_client);
    }

    // Related Error
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    pub fn update_error(&mut self, error: anyhow::Error) {
        self.error = Some(error)
    }

    pub fn own_error(&mut self) -> Option<anyhow::Error> {
        self.error.take()
    }

    pub fn has_exceed_max_frame_size(&self, payload_len: usize) -> bool {
        self.http2_options_from_me.get_max_frame_size() > payload_len as u32
    }

}


pub struct ConnectionContextBuilder<T, C> {
    req_context: Option<T>,
    // PhantomData는 실제 데이터를 가지지는 않지만, 이 구조체 타입이 마치 'C' 타입을 가지고 있는 것처럼 컴파일러에게 알려줌.
    // 이렇게 되면 <T, C>를 실제 타입에서 사용할 수 있게 됨.
    _marker: PhantomData<C>,
}

// *** 빌더 패턴을 사용하는 이유 ***
// 러스트에서는 모든 필드를 한번에 초기화해야한다.
// 따라서 아직, 모르는 값을 나중에 채우는 방식은 안되기 때문에 Builder 패턴을 쓴다.

// *** Generic Trait을 사용하지 못한 이유 ***
// 처음에는 ConnectionContextBuilder의 공통된 부분을 Generic Trait으로 빼려고 했다.
// Generic Trait은 구조체의 필드에 안정적으로 접근할 수 있는 방법이 없다.
// Builder 패턴은 From을 이용해서 만드는 것이 좋다.

// *** ConnectionContextBuilder를 굳이 2개로 나눈 이유 ***
// Bound가 필요한 영역, 아닌 영역을 나누기 위함이다.
impl <T, C> ConnectionContextBuilder<T, C> {

    pub fn new() -> Self {
        Self { req_context: None, _marker: PhantomData }
    }

    pub fn req_ctx(&mut self, req_ctx: T) -> &mut Self {
        self.req_context.replace(req_ctx);
        self
    }

}

impl <T, C> ConnectionContextBuilder<T, C>
where
    C: From<T>
{
    pub fn build(self) -> anyhow::Result<C> {
        let req_ctx = self.req_context
            .ok_or_else(|| anyhow!("missing http request context"))?;
        Ok(C::from(req_ctx))
    }
}