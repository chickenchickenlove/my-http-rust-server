use std::ascii::AsciiExt;
use std::collections::{HashMap, HashSet, VecDeque};
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
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http2::http2_header_decoder::Http2HeaderDecoder;
use crate::http2::http2_reader::ReaderTask;
use crate::http2::http2_stream::{ActiveStreamCountCommand, ReaderChannelMessage};
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
                    println!("Error occured {:?}. client may got receive response from server.", e);
                }
                if !should_keep_alive {
                    println!("should close");
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
                    println!("Error occured {:?}. client may got receive response from server.", e);
                }

                if !should_keep_alive {
                    println!("should close");
                    break
                }

            }
            else {
                break;
            }
        }
    }

    async fn loop_http2(&self,
                         mut owner: ConnectionOwner,
                         mut conn_ctx: Http2ConnectionContext1,
                         preface_message: PrefaceMessage) -> anyhow::Result<()>
    {

        // let mut error: Option<anyhow::Error> = None;

        // TODO: clean up terminated streams.
        // TODO: Handle Pending Frames because of window size.
        let (mut reader, mut writer) = owner.into_parts();

        let mut decoder = Http2HeaderDecoder::new(Decoder::new());
        decoder.set_max_table_size(4096);
        decoder.set_max_allowed_table_size(4096);

        let mut handler = Http2Handler::new();
        handler.handle_preface(&preface_message.0, &mut reader)
            .await?;

        // Send Settings Frame.
        let settings_frame = SettingsFrameFacade::new(
            SettingsFrameFacade::encode_payload(SettingsFrameFacade::default_payload())?
        );
        send(SettingsFrame(settings_frame), &mut writer).await?;

        // Ack Settings Frame from clients.
        let (mut settings_client_frame, settings_ack_frame) = handler.handle_settings(&mut conn_ctx, &mut reader).await?;
        send(settings_ack_frame, &mut writer).await?;

        // Set Connection Level Window Size and frame queue.
        let init_window_size = settings_client_frame.window_size();
        conn_ctx.set_window_size_of_client(init_window_size);
        let mut pending_frames: VecDeque<FrameFacade> = VecDeque::new();

        // Create Channel for Stream
        let (tx_for_stream, mut rx_for_stream) = mpsc::channel::<Http2ChannelMessage>(100);
        conn_ctx.set_sender(tx_for_stream);

        // Create Channel for Reader
        let (tx_to_reader, mut rx_from_reader) = mpsc::channel::<ReaderChannelMessage>(100);

        // Create Reader and reserve.
        let reader_task = ReaderTask::with(reader, tx_to_reader, conn_ctx.get_reader_token());
        let _join_join = spawn(reader_task.serve());

        let mut end_loop = false;
        loop {

            if conn_ctx.has_error() {
                let e = conn_ctx.own_error().unwrap();

                match e.downcast::<Http2Error>() {
                    Ok(http2_error) => {
                        println!("http2_error {:?}", http2_error);
                        match http2_error {
                            Http2Error::ConnectionError(error_code) => {
                                self.handle_connection_error(error_code, &mut conn_ctx, &mut writer).await?
                            },
                            Http2Error::StreamError {stream_id, code} => {
                                self.handle_stream_error(stream_id, code, &mut conn_ctx, &mut writer).await?;
                            },
                        }
                    },
                    _ => {
                        println!("Error Occured. during downcast");
                    }
                }
            }

            if conn_ctx.should_close() {
                break
            }
            // loop를 돌면서 sleep.await() 등을 하게 하는 경우에는
            // 많은 Task들이 쓸데없이 wake up -> sleep을 반복함.
            // 특히 기존 코드에서는 Stream들이 wake up -> sleep을 무한히 반복했는데, 요청이 늘어날수록
            // Task가 많아져서 CPU 사용량이 치솟고, 필요한 시점에 채널에서 메세지를 읽어오지 못하는 일이 많이 발생했다.
            let result: anyhow::Result<()> = select! {
                Some(maybe_parsed_frame) = rx_from_reader.recv() => {
                    match maybe_parsed_frame {
                        ReaderChannelMessage::MaybeFrameParsed { frame: parsed_frame } => {
                            if !conn_ctx.is_valid_considering_inflight_headers(&parsed_frame) {
                                conn_ctx.update_error(Http2Error::ConnectionError(ErrorCode::ProtocolError).into());
                                return Ok(())
                            }

                            let result = match parsed_frame {
                                SettingsFrame(_) =>  self.handle_connection_level_frame(parsed_frame, &mut conn_ctx, &mut handler, &mut writer).await,
                                PingFrame(_) => self.handle_connection_level_frame(parsed_frame, &mut conn_ctx, &mut handler, &mut writer).await,
                                GoawayFrame(_) => self.handle_connection_level_frame(parsed_frame, &mut conn_ctx, &mut handler, &mut writer).await,
                                _ => self.handle_stream_level_frame(parsed_frame, &mut conn_ctx, &mut handler, &mut writer, init_window_size).await,
                            };

                            if let Err(e) = result {
                                conn_ctx.update_error(e)
                            }

                            Ok(())
                        },
                        _ => Ok(())
                    }
                }

                // Task2 -> Handle Request Context from Stream.
                Some(msg) = rx_for_stream.recv() => {
                    // println!("### Connection Context got Message from stream : {:?}", msg);
                    println!("### Connection Context got Message from stream");
                    match msg {
                        // Should dispatch with RequestContext.
                        Http2ChannelMessage::RequestContextForDispatch {req_ctx, stream_id} => {

                            let dis = self.dispatcher.clone();
                            let limiter = self.concurrent_dispatch_limit.clone();
                            let cancel = conn_ctx.get_stream_token_by_stream_id(stream_id);

                            if let Some(tx_to_stream) = conn_ctx.get_sender_to_channel(stream_id) {
                                let tx = tx_to_stream.clone();
                                let _join_handle = spawn(async move {
                                    let _permit = limiter.acquire().await.map_err(|_| anyhow!(("Acquire Error")));
                                    let fut = dis.dispatch(req_ctx.into());

                                    select! {
                                        // Respect cancel event.
                                        biased;

                                        _ = cancel.cancelled() => {
                                            // Stream or Connection already has gone.
                                            return
                                        }

                                        res = fut => {
                                            match res {
                                                Ok(r) => {
                                                    // Ignore Result
                                                    let _ = tx
                                                    .send(
                                                        Http2ChannelMessage::ResponseObject { response: r.into() }
                                                    )
                                                    .await;
                                                },
                                                Err(_e) => {
                                                    let res = HttpResponse::with_status_code(HttpStatus::InternalServerError);
                                                    let _ = tx
                                                    .send(
                                                        Http2ChannelMessage::ResponseObject { response: res.into() }
                                                    )
                                                    .await;
                                                }
                                            };
                                        }
                                    }
                                });
                            }

                            Ok(())
                        },

                        // Should Response to Client.
                        // However, It cannot be finished because of lack of window size.
                        Http2ChannelMessage::DoResponseToClient {frames, stream_id} => {

                            let mut overflowed = false;
                            for frame in frames {
                                if frame.is_data_frame() {
                                    let payload_size = frame.get_payload_size();
                                    if conn_ctx.current_window_size() < payload_size {
                                        overflowed = true;
                                    } else {
                                        conn_ctx.decrement_window_size(payload_size)
                                    }
                                }

                                if overflowed {
                                    pending_frames.push_back(frame);
                                } else {
                                    send(frame, &mut writer).await?;
                                }
                            }
                            Ok(())
                        },

                        // TCase #3
                        Http2ChannelMessage::DecodingHeadersRequest { stream_id, payload } => {
                            match decoder.decode_headers(stream_id, payload) {
                                Ok((headers, trailers)) => {
                                    if !headers.is_empty() {
                                        if let Some(tx) = conn_ctx.get_sender_to_channel(stream_id) {
                                            tx.send(Http2ChannelMessage::DecodingHeadersResponse { headers , error: None, maybe_trailer: trailers })
                                              .await?
                                        }
                                    }
                                    Ok(())
                                }
                                Err(e) => {
                                    conn_ctx.update_error(e);
                                    Ok(())
                                }
                            }
                        }

                        // Case #4
                        Http2ChannelMessage::DecodingTrailersRequest { stream_id, payload, trailers } => {
                            match decoder.decode_trailers(stream_id, payload, trailers) {
                                Ok(headers) => {
                                    if !headers.is_empty() {
                                        if let Some(tx) = conn_ctx.get_sender_to_channel(stream_id) {
                                            tx.send(Http2ChannelMessage::DecodingTrailersResponse{ headers, stream_id })
                                              .await?
                                        }
                                    }
                                    Ok(())
                                },

                                Err(e) => {
                                    conn_ctx.update_error(e);
                                    Ok(())
                                }
                            }
                        }

                        // Case #5
                        Http2ChannelMessage::Error { error: from_error } => {
                            conn_ctx.update_error(from_error);
                            Ok(())
                        }

                        // Case #6
                        Http2ChannelMessage::ActiveStreamCount(cmd, sid) => {
                            match cmd {
                                ActiveStreamCountCommand::Increment => conn_ctx.increment_active_stream_count(sid),
                                ActiveStreamCountCommand::Decrement => conn_ctx.decrement_active_stream_count(),
                                ActiveStreamCountCommand::Keep => (),
                            }
                            Ok(())
                        }


                    // Unexpected Message.
                    _ => {
                            println!("### Connection got unexpected Msg from stream : msg {:?}", msg);
                            Ok(())
                        }
                    }
                }

                _ = sleep(Duration::from_secs(5)) => {
                    end_loop = true;
                    Ok(())
                }

            };

            if let Err(e) = result {
                break;
            }


            if end_loop {
                break;
            }

        }

        println!("### Connection context closed!");
        conn_ctx.cancel_root_token();
        Ok(())
    }


    // TODO : Move this Http2Handler
    pub async fn handle_stream_error(&self,
                                     stream_id: u32,
                                     error_code: ErrorCode,
                                     conn_ctx: &mut Http2ConnectionContext1,
                                     writer: &mut BufWriter<OwnedWriteHalf>,
    ) -> anyhow::Result<()>
    {
        // TODO: 동시성 문제가 있을 수 있음. 따라서 Cancel 된 것을 무시해야한다고 Connection Process에 알려줘야 함.
        conn_ctx.cancel_stream_token(stream_id);

        let rst_stream_frame = RstStreamFrame(RstStreamFrameFacade::with_error(stream_id, error_code));
        send(rst_stream_frame, writer).await

    }

    // TODO : Move this Http2Handler
    pub async fn handle_connection_error(&self,
                                         error_code: ErrorCode,
                                         conn_ctx: &mut Http2ConnectionContext1,
                                         writer: &mut BufWriter<OwnedWriteHalf>,
    ) -> anyhow::Result<()>{
        // TODO: 동시성 문제가 있을 수 있음. 따라서 Cancel 된 것을 무시해야한다고 Connection Process에 알려줘야 함.
        conn_ctx.cancel_root_token();
        let seen_max_stream_id = conn_ctx.get_seen_max_stream_id();
        let goaway_frame = GoawayFrame(GoawayFrameFacade::with_error(seen_max_stream_id, error_code));

        send(goaway_frame, writer).await
    }


    pub async fn handle_connection_level_frame(&self,
                                               mut frame: FrameFacade,
                                               conn_ctx: &mut Http2ConnectionContext1,
                                               handler: &mut Http2Handler,
                                               writer: &mut BufWriter<OwnedWriteHalf>,
    ) -> anyhow::Result<()>{
        // TODO: Check Inflight Header Frame.
        // If so, PROTOCOL_ERROR(connection error) should be thrown.

        // Ack if need.
        let maybe_ack_frame = handler
            .handle_parsed_frame(conn_ctx, &mut frame)
            .await?;

        if let Some(ack_frame) = maybe_ack_frame {
            send(ack_frame, writer).await?;
        }

        Ok(())
    }

    pub async fn handle_stream_level_frame(&self,
                                           mut frame: FrameFacade,
                                           conn_ctx: &mut Http2ConnectionContext1,
                                           handler: &mut Http2Handler,
                                           writer: &mut BufWriter<OwnedWriteHalf>,
                                           init_window_size: u32
    ) -> anyhow::Result<()>
    {
        // TODO: Check Inflight Header Frame.
        // If so, PROTOCOL_ERROR(connection error) should be thrown.

        println!("My Frame : {:?}", frame);

        if Http2StreamHandler::should_ignore(&frame, &conn_ctx) {
            return Ok(());
        }


        // Stream Init if need.
        if Http2StreamHandler::should_init_stream(&frame, &conn_ctx) {

            //    The identifier of a newly established stream MUST be numerically
            //    greater than all streams that the initiating endpoint has opened or
            //    reserved.  This governs streams that are opened using a HEADERS frame
            //    and streams that are reserved using PUSH_PROMISE.  An endpoint that
            //    receives an unexpected stream identifier MUST respond with a
            //    connection error (Section 5.4.1) of type PROTOCOL_ERROR.
            //    ### Race Condition : If this task is handled by async in stream process,
            //        It can not ensure that keep the this rules.
            match frame {
                HeadersFrame(_) => conn_ctx.update_max_active_stream_id(frame.stream_id()),
                _ => ()
            }

            if frame.stream_id() < conn_ctx.get_max_active_stream_id() {
                conn_ctx.update_error(Http2Error::ConnectionError(ErrorCode::ProtocolError).into());
                return Ok(())
            }

            let (tx, rx) = mpsc::channel::<Http2ChannelMessage>(100);
            let sid = frame.stream_id();
            conn_ctx.add_stream(sid, tx);

            let stream = Http2Stream::with(
                sid,
                rx,
                conn_ctx.get_sender()?,
                init_window_size,
                conn_ctx.get_stream_token_by_stream_id(sid)
            );

            conn_ctx.update_seen_max_stream_id(sid);
            let _join_handle = spawn(stream.serve());
        };

        // Ack if need.
        let maybe_ack_frame = handler
            .handle_parsed_frame(conn_ctx, &mut frame)
            .await?;

        if let Some(ack_frame) = maybe_ack_frame {
            send(ack_frame, writer).await?;
        }

        let sid = frame.stream_id();
        if let Some(tx_to_stream) = conn_ctx.get_sender_to_channel(sid) {
            println!("### Try to send frame to stream. {:?}", frame);
            tx_to_stream
                .send(
                    Http2ChannelMessage::FrameForRequest { frame }
                )
                .await?;
        }
        Ok(())
    }

    pub async fn handle(&self, tcp_stream: TcpStream) -> anyhow::Result<()> {

        let mut owner = ConnectionOwner::with(tcp_stream);
        let (conn_ctx, preface_message) = owner
            .create_connection_context()
            .await?;

        match conn_ctx {
            HttpConnectionContext1::HTTP1Context(v) => self.loop_http1(owner, v, preface_message).await,
            HttpConnectionContext1::HTTP11Context(v) => self.loop_http11(owner, v, preface_message).await,
            HttpConnectionContext1::HTTP2Context(v) => self.loop_http2(owner, v, preface_message).await?
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

async fn send(frame: FrameFacade, writer: &mut BufWriter<OwnedWriteHalf>) -> anyhow::Result<()>
{
    println!("Sending frame: {:?}", frame);
    let bytes: Bytes = match frame {
        SettingsFrame(settings_frame) => {
            settings_frame.try_into()
        },
        PingFrame(ping_frame) => {
            ping_frame.try_into()
        },
        HeadersFrame(headers_frame) => {
            headers_frame.try_into()
        },
        DataFrame(data_frame) => {
            data_frame.try_into()
        },
        RstStreamFrame(rst_stream_frame) => {
            rst_stream_frame.try_into()
        },
        GoawayFrame(go_away_frame) => {
            go_away_frame.try_into()
        },
        _ => {
            bail!("Unsupported Type");
        }
     }?;

    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}