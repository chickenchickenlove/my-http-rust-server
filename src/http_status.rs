#[derive(Clone, Copy)]
pub enum HttpStatus {
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status
    // 2XX
    OK = 200,
    Created = 201,
    Accepted = 202,
    NoContent = 204,
    ResetContent = 205,

    // 3xx
    MultipleChoices = 300,
    MovedPermanently = 301,
    Found = 302,
    NotModified = 304,
    TemporaryRedirect = 307,
    PermanentRedirect = 308,

    // 4XX
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    NotAcceptable = 406,
    RequestTimeout = 408,
    Gone = 410,
    LengthRequired = 411,
    ContentTooLarge = 413,
    UnSupportedMediaType = 415,
    TooEarly = 425,
    TooManyRequests = 429,

    // 5XX
    InternalServerError = 500,
    NotImplemented = 501,
    BadGateway = 502,
    ServiceUnavailable = 503,
    GatewayTimeout = 504,

}


impl From<HttpStatus> for u16 {
    fn from(value: HttpStatus) -> Self {
        match value {
            // 2xx
            HttpStatus::OK => 200u16,
            HttpStatus::Created => 201u16,
            HttpStatus::Accepted => 202u16,
            HttpStatus::NoContent => 204u16,
            HttpStatus::ResetContent => 205u16,

            // 3xx
            HttpStatus::MultipleChoices => 300u16,
            HttpStatus::MovedPermanently => 301u16,
            HttpStatus::Found => 302u16,
            HttpStatus::NotModified => 304u16,
            HttpStatus::TemporaryRedirect => 307u16,
            HttpStatus::PermanentRedirect => 308u16,

            // 4xx
            HttpStatus::BadRequest => 400u16,
            HttpStatus::Unauthorized => 401u16,
            HttpStatus::Forbidden => 403u16,
            HttpStatus::NotFound => 404u16,
            HttpStatus::MethodNotAllowed => 405u16,
            HttpStatus::NotAcceptable => 406u16,
            HttpStatus::RequestTimeout => 408u16,
            HttpStatus::Gone => 410u16,
            HttpStatus::LengthRequired => 411u16,
            HttpStatus::ContentTooLarge => 413u16,
            HttpStatus::UnSupportedMediaType => 415u16,
            HttpStatus::TooEarly => 425u16,
            HttpStatus::TooManyRequests => 429u16,

            // 5xx
            HttpStatus::InternalServerError => 500u16,
            HttpStatus::NotImplemented => 501u16,
            HttpStatus::BadGateway => 502u16,
            HttpStatus::ServiceUnavailable => 503u16,
            HttpStatus::GatewayTimeout => 504u16,
        }
    }
}

impl From<HttpStatus> for String {
    fn from(value: HttpStatus) -> Self {
        match value {
            // 2xx
            HttpStatus::OK => "OK".to_string(),
            HttpStatus::Created => "Created".to_string(),
            HttpStatus::Accepted => "Accepted".to_string(),
            HttpStatus::NoContent => "No Content".to_string(),
            HttpStatus::ResetContent => "Reset Content".to_string(),

            // 3xx
            HttpStatus::MultipleChoices => "Multiple Choices".to_string(),
            HttpStatus::MovedPermanently => "Moved Permanently".to_string(),
            HttpStatus::Found => "Found".to_string(),
            HttpStatus::NotModified => "Not Modified".to_string(),
            HttpStatus::TemporaryRedirect => "Temporary Redirect".to_string(),
            HttpStatus::PermanentRedirect => "Permanent Redirect".to_string(),

            // 4xx
            HttpStatus::BadRequest => "Bad Request".to_string(),
            HttpStatus::Unauthorized => "Unauthorized".to_string(),
            HttpStatus::Forbidden => "Forbidden".to_string(),
            HttpStatus::NotFound => "Not Found".to_string(),
            HttpStatus::MethodNotAllowed => "Method Not Allowed".to_string(),
            HttpStatus::NotAcceptable => "Not Acceptable".to_string(),
            HttpStatus::RequestTimeout => "Request Timeout".to_string(),
            HttpStatus::Gone => "Gone".to_string(),
            HttpStatus::LengthRequired => "Length Required".to_string(),
            HttpStatus::ContentTooLarge => "Content Length Large".to_string(),
            HttpStatus::UnSupportedMediaType => "Unsupported Media Type".to_string(),
            HttpStatus::TooEarly => "Too Early".to_string(),
            HttpStatus::TooManyRequests => "Too Many Requests".to_string(),

            // 5xx
            HttpStatus::InternalServerError => "Internal Server Error".to_string(),
            HttpStatus::NotImplemented => "Not Implemented".to_string(),
            HttpStatus::BadGateway => "Bad Gateway".to_string(),
            HttpStatus::ServiceUnavailable => "Service Unavailable".to_string(),
            HttpStatus::GatewayTimeout => "Gateway Timeout".to_string(),
        }
    }
}
