use std::collections::HashMap;
use std::os::macos::raw::stat;
use crate::handler::Http1ConnectionContext;
use crate::http_type::HttpProtocol;
use crate::dispatcher::Method;
use crate::http_status::HttpStatus;

pub struct HttpRequest {
    pub method: Method,
    pub path: String,
    pub protocol: HttpProtocol,
    pub headers: HashMap<String, String>,
    pub body: Option<String>
}


impl From<Http1ConnectionContext> for HttpRequest {
    fn from(ctx: Http1ConnectionContext) -> Self {
        let req_ctx = ctx.clone_req_ctx();
        HttpRequest {
            method: req_ctx.method(),
            path: req_ctx.path(),
            protocol: HttpProtocol::HTTP1,
            headers: req_ctx.headers(),
            body: req_ctx.body(),
        }
    }
}


pub struct HttpResponse {
    status_code: HttpStatus,
    headers: HashMap<String, String>,
    body: Option<String>
}

impl HttpResponse {
    pub fn new() -> Self {
        HttpResponse{status_code: HttpStatus::OK, headers: HashMap::new(), body: None}
    }

    pub fn set_status_code(&mut self, status_code: HttpStatus) {
        self.status_code = status_code;
    }

    pub fn get_status_code(&self) -> HttpStatus {
        self.status_code
    }

}