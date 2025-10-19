use std::collections::HashMap;
use bytes::Bytes;
use tokio::io::BufWriter;
use tokio::net::tcp::OwnedWriteHalf;
use crate::http_object::HttpResponse;

#[derive(Debug)]
pub struct Http2Response {
    headers: HashMap<String, String>,
    body: Option<Bytes>
}


impl From<HttpResponse> for Http2Response {
    fn from(value: HttpResponse) -> Self {
        let (status_code, headers, body) = value.into_parts();
        Http2Response::with(status_code.into(),
                            headers,
                            body)
    }
}


impl Http2Response {
    pub fn with(status: u16, mut headers: HashMap<String, String>, body: Option<Bytes>) -> Self {
        headers.insert(":status".to_string(), status.to_string());
        let normalized_headers: HashMap<String, String> = headers.into_iter()
            .map(|(k, v)| (k.to_lowercase(), v))
            .collect();
        Http2Response { headers: normalized_headers, body }
    }

    pub fn into_parts(self) -> (HashMap<String, String>, Option<Bytes>) {
        (self.headers, self.body)
    }
}