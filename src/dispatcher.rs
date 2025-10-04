use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpStream;
use crate::connection::{ConnectionOwner};
use crate::connection_reader::{HttpConnectionContext};
use crate::http_object::{HttpRequest, HttpResponse};

use anyhow::{bail, Result};
use crate::http_status::HttpStatus;
use crate::http_type::Method;

pub struct Dispatcher {
    router: Router
}

// TODO: 동적 디스패치없이 monomorphization으로 처리가 가능한 방법이 있는지 검토.
// 1. Arc<T>로 해야한다. -> Rc<T>로 하게 되면 tokio::spawn(...) 내에서 사용될 Handler가 !Send 이다. 그러나 Tokio::spawn(...)는 Send + 'static을 요구하기 때문에 컴파일 에러가 발생한다.
// 2. Trait Fn(...) -> ...은 dyn을 쓰지 않으면 단 하나의 타입만 의미한다. 동적 디스패치를 위해서 dyn Fn(...)을 사용한다.
// 3. Type Alias에 Generic을 선언하는 것은 타입 강제력이 약하다. 보통은 사용하는 함수/구조체에서 바운드를 직접 거는 형식이 된다. Type Bound를 타입 자체에 넣고 싶다면 Trait Object를 사용한다.
// 4. 아래 Trait Object는 Send + Sync도 구현해야한다는 것을 의미한다. 그리고 다양한 Trait Object Fn이 올 수 있음을 의미한다.

pub type Handler = Arc<
    dyn Fn(HttpRequest, HttpResponse)
        -> Pin<Box<dyn Future<Output = Result<HttpResponse>> + Send + 'static>>
    + Send
    + Sync
    // + 'static
>;



impl Dispatcher {
    pub fn new() -> Dispatcher {
        Dispatcher { router : Router::new() }
    }

    pub fn add(&mut self, method: Method, path: &str, handler: Handler) -> Result<()>
    {
        Ok(self.router.add(method, path, handler)?)
    }

    // async dispatch(...)에서 &self가 들어온다. 그리고 이 함수는 spawn(...)에 의해서 생성되는데,
    // spawn(...)은 내부에 있는 모든 인자가 Send + Sync를 구현한 것을 요구한다.
    // 그리고 dispatcher는 여러곳에서 소유하고
    pub async fn dispatch(&self, tcp_stream: TcpStream) -> Result<()> {
        let mut owner = ConnectionOwner::new(tcp_stream);
        let conn_context = owner.handle().await?;

        let res: Result<HttpResponse> = match conn_context {
            HttpConnectionContext::HTTP1Context(ctx) => {
                let req: HttpRequest = ctx.clone_req_ctx().into();
                if let Some(handler) = self.router.find(req.method, req.path.as_str()) {
                    handler(req, HttpResponse::new()).await
                }
                else {
                    Ok(HttpResponse::with_status_code(HttpStatus::NotFound))
                }
            }
        };

        let res_future = match res {
            Ok(r) => {
                owner.response(r)
            },
            Err(_e) => {
                let mut r = HttpResponse::new();
                r.set_status_code(HttpStatus::InternalServerError);
                owner.response(r)
            }
        };

        if let Err(e) = res_future.await {
            println!("Error occured {:?}. client may got receive response from server.", e);
        }

        Ok(())
    }
}


struct Router {
    // key : path, value : handler
    get_routes: HashMap<String, Handler>,
    post_routes: HashMap<String, Handler>,
    put_routes: HashMap<String, Handler>,
    delete_routes: HashMap<String, Handler>,
}

impl Router {

    pub fn new() -> Router {
        Router {
            get_routes: HashMap::new(),
            post_routes: HashMap::new(),
            put_routes: HashMap::new(),
            delete_routes: HashMap::new(),
        }
    }

    pub fn find(&self, method: Method, path: &str) -> Option<&Handler> {
        let maybe_routes = match method {
            Method::GET => Some(&self.get_routes),
            Method::POST => Some(&self.post_routes),
            Method::PUT => Some(&self.put_routes),
            Method::DELETE => Some(&self.delete_routes),
        };

        if let Some(routes) = maybe_routes {
            routes.get(path)
        }
        else {
            None
        }
    }

    pub fn add(&mut self, method: Method, path: &str, handler: Handler) -> Result<()>
    {
        if !path.starts_with("/") {
            // API를 제공하는 입장이기 때문에 Result를 반환해서 에러를 처리하도록 선택지를 주는 것이 낫다.
            bail!("Invalid path: {}. path should starts with '/'", path);
        }

        let routes = self.find_route_by_method(method)?;
        Self::add_(routes, path, handler)?;
        Ok(())
    }

    fn find_route_by_method(&mut self, method: Method) -> Result<&mut HashMap<String, Handler>>
    where Method: Debug
    {
        match method {
            Method::GET    => Ok(&mut self.get_routes),
            Method::POST   => Ok(&mut self.post_routes),
            Method::PUT    => Ok(&mut self.put_routes),
            Method::DELETE => Ok(&mut self.delete_routes),
            _              => bail!("failed to find handler to dispatch for method: {:?}", method)
        }
    }

    fn add_(routes: &mut HashMap<String, Handler>, path: &str, handler: Handler) -> Result<()>
    {
        if routes.contains_key(path) {
            let msg = format!("{} already exists, so it will be overrided.", path);
            bail!(msg)
        }
        routes.insert(path.to_string(), handler);
        Ok(())
    }

}

// 이미 &mut self인 상태에서 다시 한번 self.add_()를 호출할 수 없다.
// 왜냐하면 &mut self를 빌린 상태에서 self.add_()를 호출하면 shared reference, mutable reference를 한번 더 빌리는 것이기 때문이다.