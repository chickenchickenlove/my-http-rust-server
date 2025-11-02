use std::collections::{HashMap, HashSet};
use anyhow::anyhow;
use bytes::{Bytes, BytesMut};
use tracing::debug;
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::common_frame_facade::FrameFacade::{ContinuationFrame, DataFrame, HeadersFrame, PingFrame, RstStreamFrame, WindowUpdateFrame};
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http2::http2_stream::{DecodingHeaderState, Http2ChannelMessage};
use crate::http2::stream::stream::Stream;
use crate::http_request_context::{Http2RequestContext, RequestContextBuilder};

pub struct Http2StreamHandler2 {}


impl Http2StreamHandler2 {

    pub fn new() -> Self {
        Http2StreamHandler2 {}
    }

    // 필요한 경우, 여기서 decode 요청을 보낼 수도 있음.
    // 그런데 여기서 decode 요청을 보내면, 좀 복잡하지 않나?
    // frame을 받았을 때 반환 값은 Stream이 되어야 함.
    // Frame을 받은 경우
    pub fn handle_frame(self, mut stream: Stream, frame: FrameFacade) -> anyhow::Result<Stream> {
        stream.validate_whether_handle_headers_or_continuation_frame(&frame)?;

        let mut new_state_stream = stream.maybe_next_state_from_me(&frame)?;

        // TODO: 이 부분은 Stream이 HalfClosedRemoted, Closed 인지로 확인하면 될 것 같음.
        // if frame.has_end_stream_flag() {
        //     self.request_end = true;
        // }

        let stream_ctx = new_state_stream.context_as_mut_ref();
        let stream_id = stream_ctx.stream_id();

        match frame {
            HeadersFrame(_) if stream_ctx.handle_trailers_in_progress() => {
                if frame.has_end_headers_flag() {
                    stream_ctx.update_decoding_trailers_state(DecodingHeaderState::Ready);
                } else {
                    stream_ctx.update_decoding_trailers_state(DecodingHeaderState::KeepGoing);
                }
                stream_ctx.add_inflight_trailers_headers(frame)
            },
            HeadersFrame(_) => {
                if frame.has_end_headers_flag() {
                    stream_ctx.update_decoding_headers_state(DecodingHeaderState::Ready);
                } else {
                    stream_ctx.update_decoding_headers_state(DecodingHeaderState::KeepGoing);
                }

                stream_ctx.add_inflight_headers(frame)
            }

            ContinuationFrame(_) => {
                if matches!(stream_ctx.decoding_header_state(), DecodingHeaderState::Inflight) ||
                    matches!(stream_ctx.decoding_header_state(), DecodingHeaderState::Completed)
                {
                    if frame.has_end_headers_flag() {
                        stream_ctx.update_decoding_trailers_state(DecodingHeaderState::Ready);
                    } else {
                        stream_ctx.update_decoding_trailers_state(DecodingHeaderState::KeepGoing);
                    }
                    stream_ctx.add_inflight_trailers_headers(frame);
                } else {
                    if frame.has_end_headers_flag() {
                        stream_ctx.update_decoding_headers_state(DecodingHeaderState::Ready);
                    } else {
                        stream_ctx.update_decoding_headers_state(DecodingHeaderState::KeepGoing);
                    }
                    stream_ctx.add_inflight_headers(frame)
                }
            },
            DataFrame(_) => {
                stream_ctx.add_inflight_body(frame);
            }
            WindowUpdateFrame(mut frame) => {
                let window_increment = frame.decode_payload_from()? as u64;
                if !stream_ctx.valid_window_size(window_increment) {
                    return Err(
                        Http2Error::stream_error(stream_id, ErrorCode::FlowControlError).into()
                    )
                }

                stream_ctx.increment_window(window_increment);
            },
            RstStreamFrame(_) => {}
            PingFrame(_) => {}

            _ => (),
        }

        // 외부로 옮기는게 좋을 것 같음.
        // self.maybe_composite_headers().await?;
        // self.maybe_composite_trailers().await?;
        // self.maybe_dispatch_if_needed().await?;


        // 기본적으로는 Frame이 올 때 까지 기다림.
        // Frame이 찼으면, Decoding Headers, Trailers을 하면 됨.
        // 그리고 Body Frame도 처리하면 됨.

        // Request가 완성되었다면 dispatch 하면 됨.
        // dispatch 결과를 받았다면, 다시 호출하면 됨. 그 사이에 Frame이 올 수도 있음.
        // Closed 상태가 되면, 조금 유지하면 됨.
        Ok(new_state_stream)

        // Headers / Trailers를 받은 경우
        // Request Object를 받은 경우
    }


    pub fn should_composite_headers(&self, stream: &mut Stream) -> bool {
        matches!(
            stream.context_as_mut_ref().decoding_header_state(),
            DecodingHeaderState::Ready
        )
    }


    pub fn get_headers_bytes(&mut self, stream: &mut Stream) -> anyhow::Result<Bytes>
    {

        // if !matches!(ctx.decoding_header_state(), DecodingHeaderState::Ready) {
        //     return Ok(());
        // }

        let ctx = stream.context_as_mut_ref();
        ctx.update_decoding_headers_state(DecodingHeaderState::Inflight);

        let mut header_fragments = BytesMut::new();
        while let Some(mut frame) = ctx.pop_inflight_header_frame() {
            let bytes = match frame {
                HeadersFrame(mut f) => f.get_header_fragment(),
                ContinuationFrame(mut f) => f.get_header_fragment(),
                // TODO: Custom Error로 변환.
                _ => Err(anyhow!("unexpected frame type"))
            }?;
            header_fragments.extend_from_slice(&bytes);
        }

        Ok(header_fragments.freeze())
    }

    pub fn update_headers(&mut self, stream: &mut Stream, headers: HashMap<String, String>, trailers: HashSet<String>) {

    }


    pub fn should_composite_trailers(&mut self, stream: &mut Stream) -> bool {
        let ctx = stream.context_as_mut_ref();
        ctx.is_request_end() &&
        matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Ready) &&
        matches!(ctx.decoding_header_state(), DecodingHeaderState::Completed)
    }


    pub fn composite_trailers(&mut self, stream: &mut Stream) -> anyhow::Result<(Bytes, Option<HashSet<String>>)>
    {
        // if self.request_end &&
        //     matches!(self.decoding_trailers_state, DecodingHeaderState::Ready) &&
        //     matches!(self.decoding_header_state, DecodingHeaderState::Completed)
        // {

        let ctx = stream.context_as_mut_ref();
        let mut header_fragments = BytesMut::new();

        ctx.update_decoding_trailers_state(DecodingHeaderState::Inflight);
        while let Some(mut frame) = ctx.pop_inflight_trailer_frame() {
            let bytes = match frame {
                HeadersFrame(mut f) => f.get_header_fragment(),
                ContinuationFrame(mut f) => f.get_header_fragment(),
                _ => Err(anyhow!("unexpected frame type"))
            }?;
            header_fragments.extend_from_slice(&bytes);
        }

        let payload = header_fragments.freeze();
        let trailers = ctx.trailers();

        Ok((payload, trailers))
        // self.sender_to_conn
        //     .send(
        //         Http2ChannelMessage::DecodingTrailersRequest {
        //             payload: header_fragments.freeze(),
        //             trailers: self.maybe_trailers.as_ref().unwrap().clone(),
        //             stream_id: self.stream_id
        //         }
        //     )
        //     .await?;
        // }
        // Ok(())
    }

    pub fn should_dispatch(&self, stream: &mut Stream) -> bool {
        let ctx = stream.context_as_mut_ref();
        ctx.is_request_end() &&
            matches!(ctx.decoding_header_state(), DecodingHeaderState::Completed) &&
            matches!(ctx.decoding_trailers_state(), DecodingHeaderState::Completed) &&
            !ctx.is_dispatched()  &&
            ctx.has_composite_header()
    }

    pub fn dispatch_if_needed(&mut self, stream: &mut Stream) -> anyhow::Result<Http2RequestContext>
    {
        // if self.request_end &&
        //     matches!(self.decoding_header_state, DecodingHeaderState::Completed) &&
        //     matches!(self.decoding_trailers_state, DecodingHeaderState::Completed) &&
        //     !self.dispatched  &&
        //     self.composited_header.is_some()
        // {

        let ctx = stream.context_as_mut_ref();

        let mut ctx_builder: RequestContextBuilder<Http2RequestContext> = RequestContextBuilder::new();
        ctx_builder.headers(ctx.take_composite_header());

        // TODO: Use composite_body();
        // TODO : Inflight Body를 모아서 하나의 Body로 만드는 부분 구현 필요.
        if let Some(f) = ctx.pop_inflight_body() {
            let body_payload = f.payload();
            ctx_builder.body(body_payload);
        }

        ctx.dispatched();

        // debug!("stream try to send a request context to connection process. {:?}", request_ctx);
        // self.send_request_ctx(request_ctx).await?
        // }

        Ok(ctx_builder.build()?)
    }
}