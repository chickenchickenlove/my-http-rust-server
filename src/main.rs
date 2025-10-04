pub mod connection;
pub mod connection_reader;
mod server;
mod dispatcher;
mod task;
mod http_type;
mod http_object;
mod http_status;
mod parse_header;
mod http_connection_context;
mod http_request_context;

use anyhow::Result;
use crate::http_type::{Method};
use crate::http_object::{HttpRequest, HttpResponse};

async fn hello_test(req: HttpRequest, mut res: HttpResponse) -> Result<HttpResponse> {
    println!("hello test");
    res.set_body("Expected This Body".to_string());
    Ok(res)
}

async fn ballo_test(req: HttpRequest, mut res: HttpResponse) -> Result<HttpResponse> {
    println!("ballo test");
    res.set_body("I don't know.".to_string());
    Ok(res)
}

#[tokio::main]
async fn main() -> Result<()> {

    // #1
    // Dispatcher를 Arc로 넘기다보니, Server를 직접 만들어서 라우팅을 추가하는게 되지 않았다.
    // 따라서 Builder로 처음에 다 준비가 되면, Server를 만들 때 Arc<Dispatcher>로 전달.

    // #2
    // Arc<Dispatcher>가 필요한 이유는, tokio::spawn(...)을 이용해 비동기 Task를 생성해서 dispatch를 하려고 하는데
    // dispatcher를 여러 Task에서 사용하기 위해서 Future + Send + 'static를 구현해야하기 때문이다.
    // 따라서 Arc<Dispatcher>를 해서 Send + Sync를 구현하도록 한다.
    // 그런데 Dispatcher → Router → HashMap<_, Box<dyn Fn()>>로 들어가면, dyn Fn()는 기본적으로 Send/Sync가 아님 → Router: !Sync → Dispatcher: !Sync → &Dispatcher: !Send → 스폰 불가.
    // dispatch(&self, ...)를 tokio::spawn에 바로 넘기면, 생성된 Future가 스택에 있는 self를 &self로 캡처(빌림)함.
    // spawn은 Future: Send + 'static을 요구하므로 스택에 의존하는 빌림이 있으면 거절됨.
    // 해결은 “소유”하도록 해야한다.
    // 즉, Arc<Dispatcher>를 값으로 async move 태스크에 옮겨 담으면 그 Future는 더 이상 외부 스택을 참조하지 않으므로 'static 요구를 만족합니다.
    // Arc를 소유권으로 캡처하면서 빌림을 없애기 때문에 그 Future가 'static이 되는것임.
    let mut server_builder = server::ServerBuilder::new();
    server_builder.host("127.0.0.1")
        .port(8080)
        .add(Method::GET, "/hello", hello_test)?
        .add(Method::GET, "/ballo", ballo_test)?;

    let mut server = server_builder.build();

    server
        .serve()
        .await
}
