use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{anyhow, bail};
use bytes::Bytes;
use fluke_hpack::Decoder;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::{select, spawn};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::sleep;
use tracing::{debug, info, warn, Instrument};
use crate::connection::{ConnectionOwner, PrefaceMessage};
use crate::connection_reader::Http2Handler;
use crate::dispatcher::Dispatcher;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::{DataFrame, GoawayFrame, HeadersFrame, PingFrame, RstStreamFrame, SettingsFrame};
use crate::http2::frame_goaway::GoawayFrameFacade;
use crate::http2::frame_rst_stream::RstStreamFrameFacade;
use crate::http2::frame_settings::SettingsFrameFacade;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http2::http2_header_decoder::Http2HeaderDecoder;
use crate::http2::http2_reader::ReaderTask;
use crate::http2::http2_stream::{new_stream_span, ActiveStreamCountCommand, Http2ChannelMessage, Http2Stream, Http2StreamHandler, ReaderChannelMessage};
use crate::http_connection_context::Http2ConnectionContext1;
use crate::http_object::HttpResponse;
use crate::http_status::HttpStatus;

pub struct Http2Connection {
    dispatcher: Arc<Dispatcher>,
    concurrent_dispatch_limit: Arc<Semaphore>,
    conn_ctx: Http2ConnectionContext1,
}


impl Http2Connection {

    pub fn with(conn_ctx: Http2ConnectionContext1,
                dispatcher: Arc<Dispatcher>
    ) -> Self {
        Http2Connection {
            conn_ctx,
            dispatcher,
            // TODO: use options. not hard-coded value.
            concurrent_dispatch_limit: Arc::new(Semaphore::new(100)),
        }
    }


    pub async fn serve(mut self,
                       owner: ConnectionOwner,
                       preface_message: PrefaceMessage)
        -> anyhow::Result<()>
    {
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
        let (mut settings_client_frame, settings_ack_frame) = handler.handle_settings(&mut self.conn_ctx, &mut reader).await?;
        send(settings_ack_frame, &mut writer).await?;

        // Set Connection Level Window Size and frame queue.
        let init_window_size = settings_client_frame.window_size();
        self.conn_ctx.set_window_size_of_client(init_window_size);
        let mut pending_frames: VecDeque<FrameFacade> = VecDeque::new();

        // Create Channel for Stream
        let (tx_for_stream, mut rx_for_stream) = mpsc::channel::<Http2ChannelMessage>(100);
        self.conn_ctx.set_sender(tx_for_stream);

        // Create Channel for Reader
        let (tx_to_reader, mut rx_from_reader) = mpsc::channel::<ReaderChannelMessage>(100);

        // Create Reader and reserve.
        let reader_task = ReaderTask::with(reader, tx_to_reader, self.conn_ctx.get_reader_token());
        let _join_join = spawn(reader_task.serve());

        let mut end_loop = false;
        loop {

            if self.conn_ctx.has_error() {
                let e = self.conn_ctx.own_error().unwrap();

                match e.downcast::<Http2Error>() {
                    Ok(http2_error) => {
                        warn!("connection encounters error {}", http2_error);
                        match http2_error {
                            Http2Error::ConnectionError(error_code) => {
                                self.handle_connection_error(error_code, &mut writer).await?
                            },
                            Http2Error::StreamError {stream_id, code} => {
                                self.handle_stream_error(stream_id, code, &mut writer).await?;
                            },
                        }
                    },
                    _ => {
                        // anyhow::Error 말고, 커스텀 에러로 바꾸면 이런 부분을 고려하지 않아도 될 듯 함.
                        debug!("connection failed to downcast error.");
                    }
                }
            }

            if self.conn_ctx.should_close() {
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
                            if !self.conn_ctx.is_valid_considering_inflight_headers(&parsed_frame) {
                                warn!("connection got unexpected frame during handling infligt headers.");
                                self.conn_ctx.update_error(Http2Error::ConnectionError(ErrorCode::ProtocolError).into());
                                return Ok(())
                            }

                            let result = match parsed_frame {
                                SettingsFrame(_) =>  self.handle_connection_level_frame(parsed_frame, &mut handler, &mut writer).await,
                                PingFrame(_) => self.handle_connection_level_frame(parsed_frame, &mut handler, &mut writer).await,
                                GoawayFrame(_) => self.handle_connection_level_frame(parsed_frame, &mut handler, &mut writer).await,
                                _ => self.handle_stream_level_frame(parsed_frame, &mut handler, &mut writer, init_window_size).await,
                            };

                            if let Err(e) = result {
                                self.conn_ctx.update_error(e)
                            }

                            Ok(())
                        },
                        _ => Ok(())
                    }
                }

                // Task2 -> Handle Request Context from Stream.
                Some(msg) = rx_for_stream.recv() => {
                    match msg {
                        // Should dispatch with RequestContext.
                        Http2ChannelMessage::RequestContextForDispatch {req_ctx, stream_id} => {

                            debug!("connection process got dispatch request from stream. msg: {:?}", req_ctx);
                            let dis = self.dispatcher.clone();
                            let limiter = self.concurrent_dispatch_limit.clone();
                            let cancel = self.conn_ctx.get_stream_token_by_stream_id(stream_id);

                            if let Some(tx_to_stream) = self.conn_ctx.get_sender_to_channel(stream_id) {
                                let tx = tx_to_stream.clone();
                                let _join_handle = spawn(async move {
                                    let _permit = limiter.acquire().await.map_err(|_| anyhow!("Acquire Error"));
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
                            debug!("connection process got response request from stream. msg: {:?}", frames);
                            let mut overflowed = false;
                            for frame in frames {
                                if frame.is_data_frame() {
                                    let payload_size = frame.get_payload_size();
                                    if self.conn_ctx.current_window_size() < payload_size {
                                        overflowed = true;
                                    } else {
                                        self.conn_ctx.decrement_window_size(payload_size)
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

                        // Case #3
                        Http2ChannelMessage::DecodingHeadersRequest { stream_id, payload } => {
                            debug!("connection process got decoding headers request from stream.");
                            match decoder.decode_headers(stream_id, payload) {
                                Ok((headers, trailers)) => {
                                    if !headers.is_empty() {
                                        if let Some(tx) = self.conn_ctx.get_sender_to_channel(stream_id) {
                                            tx.send(Http2ChannelMessage::DecodingHeadersResponse { headers , error: None, maybe_trailer: trailers })
                                              .await?
                                        }
                                    }
                                    Ok(())
                                }
                                Err(e) => {
                                    self.conn_ctx.update_error(e);
                                    Ok(())
                                }
                            }
                        }

                        // Case #4
                        Http2ChannelMessage::DecodingTrailersRequest { stream_id, payload, trailers } => {
                            debug!("connection process got decoding trailers request from stream.");
                            match decoder.decode_trailers(stream_id, payload, trailers) {
                                Ok(headers) => {
                                    if !headers.is_empty() {
                                        if let Some(tx) = self.conn_ctx.get_sender_to_channel(stream_id) {
                                            tx.send(Http2ChannelMessage::DecodingTrailersResponse{ headers, stream_id })
                                              .await?
                                        }
                                    }
                                    Ok(())
                                },

                                Err(e) => {
                                    self.conn_ctx.update_error(e);
                                    Ok(())
                                }
                            }
                        }

                        // Case #5
                        Http2ChannelMessage::Error { error: from_error } => {
                            debug!("connection process got error msg from stream. error: {}", from_error);
                            self.conn_ctx.update_error(from_error);
                            Ok(())
                        }

                        // Case #6
                        Http2ChannelMessage::ActiveStreamCount(cmd, sid) => {
                            match cmd {
                                ActiveStreamCountCommand::Increment => self.conn_ctx.increment_active_stream_count(sid),
                                ActiveStreamCountCommand::Decrement => self.conn_ctx.decrement_active_stream_count(),
                                ActiveStreamCountCommand::Keep => (),
                            }
                            Ok(())
                        }


                    // Unexpected Message.
                    _ => {
                            debug!("connection process got unexpected message from stream. msg {:?}", msg);
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

        debug!("connection process is closed.");
        self.conn_ctx.cancel_root_token();
        Ok(())
    }

    // TODO : Move this Http2Handler
    pub async fn handle_stream_error(&mut self,
                                     stream_id: u32,
                                     error_code: ErrorCode,
                                     writer: &mut BufWriter<OwnedWriteHalf>,
    ) -> anyhow::Result<()>
    {
        // TODO: 동시성 문제가 있을 수 있음. 따라서 Cancel 된 것을 무시해야한다고 Connection Process에 알려줘야 함.
        self.conn_ctx.cancel_stream_token(stream_id);

        let rst_stream_frame = RstStreamFrame(RstStreamFrameFacade::with_error(stream_id, error_code));
        send(rst_stream_frame, writer).await

    }

    // TODO : Move this Http2Handler
    pub async fn handle_connection_error(&mut self,
                                         error_code: ErrorCode,
                                         writer: &mut BufWriter<OwnedWriteHalf>,
    ) -> anyhow::Result<()>{
        // TODO: 동시성 문제가 있을 수 있음. 따라서 Cancel 된 것을 무시해야한다고 Connection Process에 알려줘야 함.
        self.conn_ctx.cancel_root_token();
        let seen_max_stream_id = self.conn_ctx.get_seen_max_stream_id();
        let goaway_frame = GoawayFrame(GoawayFrameFacade::with_error(seen_max_stream_id, error_code));

        send(goaway_frame, writer).await
    }


    pub async fn handle_connection_level_frame(&mut self,
                                               mut frame: FrameFacade,
                                               handler: &mut Http2Handler,
                                               writer: &mut BufWriter<OwnedWriteHalf>,
    ) -> anyhow::Result<()>{
        // TODO: Check Inflight Header Frame.
        // If so, PROTOCOL_ERROR(connection error) should be thrown.

        // Ack if need.
        let maybe_ack_frame = handler
            .handle_parsed_frame(&mut self.conn_ctx, &mut frame)
            .await?;

        if let Some(ack_frame) = maybe_ack_frame {
            send(ack_frame, writer).await?;
        }

        Ok(())
    }

    pub async fn handle_stream_level_frame(&mut self,
                                           mut frame: FrameFacade,
                                           handler: &mut Http2Handler,
                                           writer: &mut BufWriter<OwnedWriteHalf>,
                                           init_window_size: u32
    ) -> anyhow::Result<()>
    {
        // TODO: Check Inflight Header Frame.
        // If so, PROTOCOL_ERROR(connection error) should be thrown.
        if Http2StreamHandler::should_ignore(&frame, &self.conn_ctx) {
            return Ok(());
        }


        // Stream Init if need.
        if Http2StreamHandler::should_init_stream(&frame, &self.conn_ctx) {

            //    The identifier of a newly established stream MUST be numerically
            //    greater than all streams that the initiating endpoint has opened or
            //    reserved.  This governs streams that are opened using a HEADERS frame
            //    and streams that are reserved using PUSH_PROMISE.  An endpoint that
            //    receives an unexpected stream identifier MUST respond with a
            //    connection error (Section 5.4.1) of type PROTOCOL_ERROR.
            //    ### Race Condition : If this task is handled by async in stream process,
            //        It can not ensure that keep the this rules.
            match frame {
                HeadersFrame(_) => self.conn_ctx.update_max_active_stream_id(frame.stream_id()),
                _ => ()
            }

            if frame.stream_id() < self.conn_ctx.get_max_active_stream_id() {
                self.conn_ctx.update_error(Http2Error::ConnectionError(ErrorCode::ProtocolError).into());
                return Ok(())
            }

            let (tx, rx) = mpsc::channel::<Http2ChannelMessage>(100);
            let sid = frame.stream_id();

            let stream_span = new_stream_span(sid, init_window_size);
            self.conn_ctx.add_stream(sid, tx, stream_span.clone());

            let stream = Http2Stream::with(
                sid,
                rx,
                self.conn_ctx.get_sender()?,
                init_window_size,
                self.conn_ctx.get_stream_token_by_stream_id(sid)
            );

            self.conn_ctx.update_seen_max_stream_id(sid);
            let _join_handle = tokio::spawn(stream
                .serve()
                .instrument(stream_span)
            );

        };

        // Ack if need.
        let maybe_ack_frame = handler
            .handle_parsed_frame(&mut self.conn_ctx, &mut frame)
            .await?;

        if let Some(ack_frame) = maybe_ack_frame {
            send(ack_frame, writer).await?;
        }

        let sid = frame.stream_id();
        if let Some(tx_to_stream) = self.conn_ctx.get_sender_to_channel(sid) {
            debug!("connection try to send frame to stream : {}, frame : {:?}.", sid, frame);
            tx_to_stream
                .send(
                    Http2ChannelMessage::FrameForRequest { frame }
                )
                .await?;
        }
        Ok(())
    }

}

async fn send(frame: FrameFacade, writer: &mut BufWriter<OwnedWriteHalf>) -> anyhow::Result<()>
{
    info!("Sending frame: {:?}", frame);
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