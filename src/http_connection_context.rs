use std::marker::PhantomData;
use anyhow::anyhow;
use crate::http_type::HttpProtocol;
use crate::http_request_context::{Http1RequestContext, Http11RequestContext};


pub trait ConnectionContext {
    const PROTOCOL: HttpProtocol;

    fn should_close(&self) -> bool;
}

pub struct Http1ConnectionContext {
    req_context: Http1RequestContext,
}

pub struct Http11ConnectionContext {
    req_context: Http11RequestContext,
}

impl ConnectionContext for Http1ConnectionContext {
    const PROTOCOL: HttpProtocol = HttpProtocol::HTTP1;

    fn should_close(&self) -> bool {
        self.req_context.should_close()
    }
}

impl ConnectionContext for Http11ConnectionContext {
    const PROTOCOL: HttpProtocol = HttpProtocol::HTTP11;

    fn should_close(&self) -> bool {
        self.req_context.should_close()
    }
}

impl Http1ConnectionContext {

    pub fn new(req_context: Http1RequestContext) -> Self {
        Http1ConnectionContext {req_context}
    }

    pub fn clone_req_ctx(&self) -> Http1RequestContext {
        self.req_context.clone()
    }

}


impl Http11ConnectionContext {

    pub fn new(req_context: Http11RequestContext) -> Self {
        Http11ConnectionContext {req_context}
    }

    pub fn clone_req_ctx(&self) -> Http11RequestContext {
        self.req_context.clone()
    }

}


pub struct ConnectionContextBuilder<T, C> {
    req_context: Option<T>,
    // PhantomData는 실제 데이터를 가지지는 않지만, 이 구조체 타입이 마치 'C' 타입을 가지고 있는 것처럼 컴파일러에게 알려줌.
    // 이렇게 되면 <T, C>를 실제 타입에서 사용할 수 있게 됨.
    _marker: PhantomData<C>,
}

// *** 빌더 패턴을 사용하는 이유 ***
// 러스트에서는 모든 필드를 한번에 초기화해야한다.
// 따라서 아직, 모르는 값을 나중에 채우는 방식은 안되기 때문에 Builder 패턴을 쓴다.

// *** Generic Trait을 사용하지 못한 이유 ***
// 처음에는 ConnectionContextBuilder의 공통된 부분을 Generic Trait으로 빼려고 했다.
// Generic Trait은 구조체의 필드에 안정적으로 접근할 수 있는 방법이 없다.
// Builder 패턴은 From을 이용해서 만드는 것이 좋다.

// *** ConnectionContextBuilder를 굳이 2개로 나눈 이유 ***
// Bound가 필요한 영역, 아닌 영역을 나누기 위함이다.
impl <T, C> ConnectionContextBuilder<T, C> {

    pub fn new() -> Self {
        Self { req_context: None, _marker: PhantomData }
    }

    pub fn req_ctx(&mut self, req_ctx: T) -> &mut Self {
        self.req_context.replace(req_ctx);
        self
    }

}

impl <T, C> ConnectionContextBuilder<T, C>
where
    C: From<T>
{
    pub fn build(self) -> anyhow::Result<C> {
        let req_ctx = self.req_context
            .ok_or_else(|| anyhow!("missing http request context"))?;
        Ok(C::from(req_ctx))
    }
}

impl From<Http1RequestContext> for Http1ConnectionContext {
    fn from(value: Http1RequestContext) -> Self {
        Http1ConnectionContext::new(value)
    }
}


impl From<Http11RequestContext> for Http11ConnectionContext {
    fn from(value: Http11RequestContext) -> Self {
        Http11ConnectionContext::new(value)
    }
}