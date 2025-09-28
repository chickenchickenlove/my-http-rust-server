use anyhow::anyhow;
use crate::http_type::HttpProtocol;
use crate::http_request_context::{Http1RequestContext};


trait ConnectionContext {
    const PROTOCOL: HttpProtocol;
}

pub struct Http1ConnectionContext {
    req_context: Http1RequestContext,
}

impl ConnectionContext for Http1ConnectionContext {
    const PROTOCOL: HttpProtocol = HttpProtocol::HTTP1;
}

impl Http1ConnectionContext {

    pub fn new(req_context: Http1RequestContext) -> Self {
        Http1ConnectionContext {req_context}
    }

    pub fn clone_req_ctx(&self) -> Http1RequestContext {
        self.req_context.clone()
    }

}


// 러스트에서는 모든 필드를 한번에 초기화해야한다.
// 따라서 아직, 모르는 값을 나중에 채우는 방식은 안되기 때문에 Builder 패턴을 쓴다.
// #[derive!(Debug)]
pub struct Http1ConnectionContextBuilder {
    req_context: Option<Http1RequestContext>
}
// #[derive!(Debug)]
impl Http1ConnectionContextBuilder {
    pub fn new() -> Http1ConnectionContextBuilder {
        Http1ConnectionContextBuilder { req_context: None }
    }

    pub fn req_ctx(&mut self, req_ctx: Http1RequestContext) -> &mut Self {
        self.req_context.replace(req_ctx);
        self
    }

    pub fn build(self) -> anyhow::Result<Http1ConnectionContext> {
        let req_ctx = self.req_context
            .ok_or_else(|| anyhow!("missing http request context"))?;
        Ok(Http1ConnectionContext::new(req_ctx))
    }
}

