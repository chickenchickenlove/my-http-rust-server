#[derive(Clone, Copy)]
pub enum HttpStatus {
    // 2XX
    OK = 200,

    // 4XX
    NotFound = 400,

    // 5XX
    InternalServerError = 500
}


impl From<HttpStatus> for u16 {
    fn from(value: HttpStatus) -> Self {
        match value {
            HttpStatus::OK => 200u16,
            HttpStatus::NotFound => 404u16,
            HttpStatus::InternalServerError => 500u16,
            _ => 500u16
        }
    }
}

impl From<HttpStatus> for String {
    fn from(value: HttpStatus) -> Self {
        match value {
            HttpStatus::OK => "OK".to_string(),
            HttpStatus::NotFound => "Not Found".to_string(),
            HttpStatus::InternalServerError => "Internal Server Error".to_string(),
            _ => "Internal Server Error".to_string()
        }
    }
}
