use std::collections::HashMap;
use bytes::Bytes;
use crate::http_type::{HttpProtocol, Method};
use crate::http_status::HttpStatus;
use crate::http_type::HttpProtocol::{HTTP1, HTTP11};
use crate::http_request_context::{Http1RequestContext, Http11RequestContext};

pub struct HttpRequest {
    pub method: Method,
    pub path: String,
    pub protocol: HttpProtocol,
    pub headers: HashMap<String, String>,
    pub body: Option<Bytes>,
}

impl HttpRequest {
    fn new(method: Method, path: String, protocol: HttpProtocol, headers: HashMap<String, String>, body: Option<Bytes>) -> Self {
        HttpRequest { method, path, protocol, headers, body }
    }
}


impl From<Http1RequestContext> for HttpRequest {
    fn from(ctx: Http1RequestContext) -> Self {
        let (method, path, version, headers, body) = ctx.into_part();
        HttpRequest::new(method, path, HTTP1, headers, body)
    }
}

impl From<Http11RequestContext> for HttpRequest {
    fn from(ctx: Http11RequestContext) -> Self {
        let (method, path, version, headers, body) = ctx.into_part();
        HttpRequest::new(method, path, HTTP11, headers, body)
    }
}


pub struct HttpResponse {
    status_code: HttpStatus,
    headers: HashMap<String, String>,
    body: Option<String>
}

// Default Trait 구현은 납득 가능한 기본값이 있는 경우에 항상 구현해두는 것이 Rust Convention.
impl Default for HttpResponse {
    fn default() -> Self {
        HttpResponse{status_code: HttpStatus::OK, headers: HashMap::new(), body: None}
    }
}

impl HttpResponse {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_status_code(status_code: HttpStatus) -> Self {
        HttpResponse{status_code, ..Self::default()}
    }

    pub fn set_status_code(&mut self, status_code: HttpStatus) {
        self.status_code = status_code;
    }

    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body.replace(body.into());
    }

    pub fn get_status_code(&self) -> HttpStatus {
        self.status_code
    }

    pub fn insert_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let k = key.into();
        let v = value.into();
        self.headers.insert(k, v);
    }

    pub fn into_parts(self) -> (HttpStatus, HashMap<String, String>, Option<Bytes>) {
        let body_bytes = match self.body {
            Some(b) => Some(Bytes::from(b)),
            None => None
        };

        (self.status_code, self.headers, body_bytes)
    }

}