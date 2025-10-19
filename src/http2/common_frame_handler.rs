use fluke_h2_parse::FrameType::{
    Data,
    Headers,
    Ping,
    Priority,
    WindowUpdate,
    Settings,
    GoAway,
    Continuation,
    RstStream
};
use crate::http2::common_frame_facade::FrameFacade;
use crate::http2::frame_continuation::ContinuationFrameHandler;
use crate::http2::frame_data::DataFrameHandler;
use crate::http2::frame_goaway::GoawayFrameHandler;
use crate::http2::frame_headers::HeadersFrameHandler;
use crate::http2::frame_ping::PingFrameHandler;
use crate::http2::frame_priority::PriorityFrameHandler;
use crate::http2::frame_rst_stream::RstStreamFrameHandler;
use crate::http2::frame_settings::SettingsFrameHandler;
use crate::http2::frame_window_update::WindowUpdateFrameHandler;
use crate::http2::http2_errors::{ErrorCode, Http2Error};
use crate::http_connection_context::Http2ConnectionContext1;

pub struct FrameHandle {

}

impl FrameHandle {

    pub fn new() -> Self {
        FrameHandle {}
    }

    pub async fn handle(&self,
                        conn_ctx: &mut Http2ConnectionContext1,
                        frame: &mut FrameFacade) -> anyhow::Result<Option<FrameFacade>>
    {

        match frame.frame_type() {
            Settings(_) => {
                SettingsFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            Ping(_) => {
                PingFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            Priority => {
                PriorityFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            WindowUpdate => {
                WindowUpdateFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            Headers(_) => {
                HeadersFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            GoAway => {
                GoawayFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            Continuation(_) => {
                ContinuationFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            Data(_) => {
                DataFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            },
            RstStream => {
                RstStreamFrameHandler::new()
                    .handle(conn_ctx, frame)
                    .await
            }
            _ => Ok(None)
        }
    }
}



pub trait FrameHandler {

    async fn handle(&self,
                    conn_ctx: &mut Http2ConnectionContext1,
                    frame: &mut FrameFacade) -> anyhow::Result<Option<FrameFacade>> {
        self.common_validate(conn_ctx, frame).await?;
        self.decode_payload(conn_ctx, frame).await?;
        self.validate(conn_ctx, frame).await?;
        self.process(conn_ctx, frame).await?;
        let ack_frame = self.maybe_ack(conn_ctx, frame).await?;
        Ok(ack_frame)
    }

    async fn common_validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>{
        let sid = frame.stream_id();

        let is_even  = sid % 2 == 0;
        if sid != 0 && is_even {
            return Err(Http2Error::ConnectionError(ErrorCode::ProtocolError).into());
        }

        // Validate max Frame Size
        if frame.get_payload_size() > conn_ctx.get_max_frame_size_from_me() {
            if sid == 0 {
                return Err(Http2Error::ConnectionError(ErrorCode::FrameSizeError).into());
            }

            return match frame {
                // https://datatracker.ietf.org/doc/html/rfc7540#section-4.2
                // PUSH_PROMISE should be included.
                FrameFacade::HeadersFrame(_) => Err(Http2Error::ConnectionError(ErrorCode::FrameSizeError).into()),
                FrameFacade::SettingsFrame(_) => Err(Http2Error::ConnectionError(ErrorCode::FrameSizeError).into()),
                _ => Err(Http2Error::stream_error(sid, ErrorCode::FrameSizeError).into())
            };
        }

        Ok(())
    }


    async fn decode_payload(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &mut FrameFacade) -> anyhow::Result<()>;
    async fn validate(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>;
    async fn maybe_ack(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<Option<FrameFacade>>;
    async fn process(&self, conn_ctx: &mut Http2ConnectionContext1, frame: &FrameFacade) -> anyhow::Result<()>;

}