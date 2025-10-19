use thiserror::Error;
use crate::http2::http2_stream::Http2Stream;

// Error handling : https://datatracker.ietf.org/doc/html/rfc7540#section-5.4
// Error Code : https://datatracker.ietf.org/doc/html/rfc7540#section-7
#[derive(Debug, Error)]
pub enum Http2Error {
    #[error("connection error: {0:?}")]
    ConnectionError(ErrorCode),

    #[error("stream {stream_id} error: {code:?}")]
    StreamError{
        stream_id: u32,
        code: ErrorCode,
    },
}

impl Http2Error {

    pub fn stream_error(stream_id: impl Into<u32>, code: ErrorCode) -> Self {
        Http2Error::StreamError {
            stream_id: stream_id.into(),
            code,
        }
    }

    pub fn connection_error(code: ErrorCode) -> Self {
        Http2Error::ConnectionError(code)
    }
}


#[derive(Debug, Copy, Clone)]
pub enum ErrorCode {
    NoError,
    ProtocolError,
    InternalError,
    FlowControlError,
    SettingsTimeout,
    StreamClosed,
    FrameSizeError,
    RefusedStream,
    Cancel,
    CompressionError,
    ConnectError,
    EnhanceYourCalm,
    InadequateSecurity,
    Http_1_1_Required,
}


impl From<ErrorCode> for u32 {
    fn from(value: ErrorCode) -> Self {
        match value {
            ErrorCode::NoError => 0x0,
            ErrorCode::ProtocolError => 0x1,
            ErrorCode::InternalError => 0x2,
            ErrorCode::FlowControlError => 0x3,
            ErrorCode::SettingsTimeout => 0x4,
            ErrorCode::StreamClosed => 0x5,
            ErrorCode::FrameSizeError => 0x6,
            ErrorCode::RefusedStream => 0x7,
            ErrorCode::Cancel => 0x8,
            ErrorCode::CompressionError => 0x9,
            ErrorCode::ConnectError => 0xa,
            ErrorCode::EnhanceYourCalm => 0xb,
            ErrorCode::InadequateSecurity => 0xc,
            ErrorCode::Http_1_1_Required => 0xd,
        }
    }
}