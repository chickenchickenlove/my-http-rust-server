use std::collections::HashMap;
use tokio::net::TcpStream;
use crate::connection::{ConnectionOwner};
use crate::handler::{HttpConnectionContext};
use crate::http_object::{HttpRequest, HttpResponse};

use anyhow::{bail, Result};
use crate::http_status::HttpStatus;
use crate::http_status::HttpStatus::OK;

pub struct Dispatcher {
    router: Router
}

pub type Handler = fn(HttpRequest, HttpResponse) -> Result<HttpResponse>;

impl Dispatcher {
    pub fn new() -> Dispatcher {
        Dispatcher { router : Router::new() }
    }

    pub fn add(&mut self, method: Method, path: &str, handler: Handler) -> () {
        self.router.add(method, path, handler);
    }

    // async dispatch(...)에서 &self가 들어온다. 그리고 이 함수는 spawn(...)에 의해서 생성되는데,
    // spawn(...)은 내부에 있는 모든 인자가 Send + Sync를 구현한 것을 요구한다.
    // 그리고 dispatcher는 여러곳에서 소유하고
    pub async fn dispatch(&self, tcp_stream: TcpStream) -> Result<()> {
        let mut owner = ConnectionOwner::new(tcp_stream);
        let conn_context = owner.handle().await?;
        let res = match conn_context {
            HttpConnectionContext::HTTP1Context(ctx) => {
                let req: HttpRequest = ctx.into();
                if let Some(handler) = self.router.find(req.method.clone(), req.path.as_str()) {
                    handler(req, HttpResponse::new())
                }
                else {
                    let mut res = HttpResponse::new();
                    res.set_status_code(HttpStatus::NotFound);
                    Ok(res)
                }
            }
        };

        match res {
            Ok(r) => {
                owner.response(r).await;
            },
            Err(e) => {
                let mut r = HttpResponse::new();
                r.set_status_code(HttpStatus::InternalServerError);
                owner.response(r).await;
            }

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

#[derive(Copy, Clone)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
    UNSUPPORTED,
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
            Method::UNSUPPORTED => None,
        };

        if let Some(routes) = maybe_routes {
            routes.get(path)
        }
        else {
            None
        }
    }

    pub fn add(&mut self, method: Method, path: &str, handler: Handler) -> Result<()> {
        if !path.starts_with("/") {
            // API를 제공하는 입장이기 때문에 Result를 반환해서 에러를 처리하도록 선택지를 주는 것이 낫다.
            bail!("Invalid path: {}. path should starts with '/'", path);
        }

        let maybe_routes = match method {
            Method::GET    => Some(&mut self.get_routes),
            Method::POST   => Some(&mut self.post_routes),
            Method::PUT    => Some(&mut self.put_routes),
            Method::DELETE => Some(&mut self.delete_routes),
            _              => None
        };

        if let Some(routes) = maybe_routes {
            Self::add_(routes, path, handler);
        }
        Ok(())
    }

    fn find_route_by_method(&mut self, method: Method) -> Option<&mut HashMap<String, Handler>> {
        match method {
            Method::GET    => Some(&mut self.get_routes),
            Method::POST   => Some(&mut self.post_routes),
            Method::PUT    => Some(&mut self.put_routes),
            Method::DELETE => Some(&mut self.delete_routes),
            _              => None
        }
    }

    fn add_(routes: &mut HashMap<String, Handler>, path: &str, handler: Handler) -> Result<()>{
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