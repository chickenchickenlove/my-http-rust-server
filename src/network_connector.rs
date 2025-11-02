use std::ascii::AsciiExt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ptr::hash;
use std::sync::{
    Arc
};
use std::time::Duration;
use anyhow::{anyhow, bail};
use bytes::{
    Buf,
    Bytes,
    BytesMut
};
use fluke_hpack::Decoder;
use fluke_hpack::decoder::DecoderError;
use tokio::io::{
    AsyncWriteExt,
    BufWriter
};
use tokio::net::TcpStream;
use tokio::{
    select,
    spawn
};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, field, info, info_span, warn, Instrument, Span};
use tracing::field::debug;
use crate::connection::{
    ConnectionOwner,
    PrefaceMessage
};
use crate::connection_reader::{
    GeneralHttp1Handler1,
    Http11Handler,
    Http1Handler,
    Http2Handler,
    HttpConnectionContext1
};
use crate::dispatcher::Dispatcher;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::{DataFrame, GoawayFrame, HeadersFrame, PingFrame, RstStreamFrame, SettingsFrame};
use crate::http2::frame_goaway::GoawayFrameFacade;
use crate::http2::frame_rst_stream::RstStreamFrameFacade;
use crate::http2::frame_settings::SettingsFrameFacade;
use crate::http2::http2_connection2::Http2Connection2;
use crate::http2::http2_connection::Http2Connection;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http2::http2_header_decoder::Http2HeaderDecoder;
use crate::http2::http2_reader::ReaderTask;
use crate::http2::http2_stream::{new_stream_span, ActiveStreamCountCommand, ReaderChannelMessage};
use crate::http2_stream::{
    Http2ChannelMessage,
    Http2Stream,
    Http2StreamHandler
};
use crate::http_connection_context::{
    ConnectionContext,
    Http11ConnectionContext1,
    Http1ConnectionContext1,
    Http2ConnectionContext1
};
use crate::http_object::{HttpRequest, HttpResponse};
use crate::http_status::HttpStatus;

pub struct NetworkConnector {
    dispatcher: Arc<Dispatcher>,
    concurrent_dispatch_limit: Arc<Semaphore>,

}

// ConnectionContext가 하나 만들어지면... 그걸 계속 쓴다.
// 그러면 Request Context와 Connection Context는 굳이 관계를 가질 필요가 없다.
// Connection Owner 안에서 Request Context를 관리하면 되는 것이다...
// 각 Request Context마다 완성이 되면 다음 작업을 하도록 하는게 맞을 듯...
impl NetworkConnector {

    pub fn with(dispatcher: Arc<Dispatcher>) -> Self {
        NetworkConnector {
            dispatcher: dispatcher.clone(),
            concurrent_dispatch_limit: Arc::new(Semaphore::new(128)),
        }
    }

    async fn loop_http1(&self,
                        mut owner: ConnectionOwner,
                        conn_ctx: Http1ConnectionContext1,
                        preface_message: PrefaceMessage
    ) {
        let mut handler = Http1Handler::new();
        let mut preface_msg = Some(preface_message);

        loop {
            // 첫턴에 Move가 된다. -> take()를 쓴다.
            if let Ok(req_ctx) = handler.handle_(preface_msg.take(), owner.reader()).await {
                let should_keep_alive = !req_ctx.should_close();
                let dis = self.dispatcher.clone();

                let res_future = match dis.dispatch(req_ctx.into()).await {
                    Ok(r) => {
                        owner.response(r, !should_keep_alive)
                    },
                    Err(_e) => {
                        let mut r = HttpResponse::new();
                        r.set_status_code(HttpStatus::InternalServerError);
                        owner.response(r, !should_keep_alive)
                    }
                };

                if let Err(e) = res_future.await {
                    warn!("unexpected error occurred during dispatch, error: {}. client may got receive response from server", e);
                }
                if !should_keep_alive {
                    break
                }
            } else {
                break;
            }
        }
    }

    async fn loop_http11(&self,
                        mut owner: ConnectionOwner,
                        conn_ctx: Http11ConnectionContext1,
                        preface_message: PrefaceMessage) {

        let mut handler = Http11Handler::new();
        let mut preface_msg = Some(preface_message);

        loop {
            // 첫턴에 Move가 된다. -> take()를 쓴다.
            if let Ok(req_ctx) = handler.handle_(preface_msg.take(), owner.reader()).await {
                let mut should_keep_alive = !req_ctx.should_close();

                let dis = self.dispatcher.clone();
                let res_future = match dis.dispatch(req_ctx.into()).await {
                    Ok(r) => {
                        owner.response(r, !should_keep_alive)
                    },
                    Err(_e) => {
                        let mut r = HttpResponse::new();
                        r.set_status_code(HttpStatus::InternalServerError);
                        owner.response(r, !should_keep_alive)
                    }
                };

                if let Err(e) = res_future.await {
                    warn!("unexpected error occurred during dispatch, error: {}. client may got receive response from server", e);
                }

                if !should_keep_alive {
                    break
                }

            }
            else {
                break;
            }
        }
    }


    // async fn loop_http2(&self,
    //                      mut owner: ConnectionOwner,
    //                      mut conn_ctx: Http2ConnectionContext1,
    //                      preface_message: PrefaceMessage)
    //     -> anyhow::Result<()>
    // {
    //     let mut http2_conn = Http2Connection::with(conn_ctx, self.dispatcher.clone());
    //     http2_conn.serve(owner, preface_message).await
    // }

    async fn loop_http2(&self,
                        mut owner: ConnectionOwner,
                        mut conn_ctx: Http2ConnectionContext1,
                        preface_message: PrefaceMessage)
                        -> anyhow::Result<()>
    {
        let mut http2_conn = Http2Connection2::with(conn_ctx, self.dispatcher.clone());
        http2_conn.serve(owner, preface_message).await
    }

    pub async fn handle(&self, tcp_stream: TcpStream) -> anyhow::Result<()> {

        let mut owner = ConnectionOwner::with(tcp_stream);
        let (conn_ctx, preface_message) = owner
            .create_connection_context()
            .await?;

        let mut hasher = DefaultHasher::new();
        "connection".hash(&mut hasher);
        let conn_id: u64 = hasher.finish();

        let conn_span = info_span!("connection", conn_id=conn_id);
        match conn_ctx {
            HttpConnectionContext1::HTTP1Context(v) => self
                .loop_http1(owner, v, preface_message)
                .instrument(conn_span)
                .await,
            HttpConnectionContext1::HTTP11Context(v) => self
                .loop_http11(owner, v, preface_message)
                .instrument(conn_span)
                .await,
            HttpConnectionContext1::HTTP2Context(v) => self
                .loop_http2(owner, v, preface_message)
                .instrument(conn_span)
                .await?
        }

        // HTTP/1, HTTP/1.1/ HTTP/2 마다 서로 다른 loop를 타도록 일단 만들자.
        // 그리고 나중에 공통화 할 수 있는 부분이 생기면 그 때 처리한다.

        // Stream ID가 여러개 동시에 올 수 있고, 그것이 각각 RequestContextHandler가 될 수 있다.
        // Stream이 완성되는대로 RequestContext를 만들고, 그걸 dispatch에게서 처리하도록 하는게 나을 것 같기도 하다.
        //
        // 1. RequestContext를 만드는 것 따로
        // 2. 만들어진 RequestContext를 처리하는 것 따로. (여기서 async spawn을 다시 하는게 나을 것 같음.

        // HTTP/1은 loop를 돌면 안됨.
        // HTTP/1.1은 loop를 돌긴 해야하는데 응답을 다 받은 후에 종료해야 함.
        // HTTP/2는 계속 루프를 돌아야 함.
        // Preface Message 관점
        // 1. HTTP/1, HTTP/1.1은 매번 Preface Message가 생성됨.
        // 2. HTTP2는 한번 생성된 이후에 나타나지 않음.

        Ok(())
    }


}