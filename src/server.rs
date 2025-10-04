use std::future::Future;
use std::sync::Arc;
use crate::dispatcher::{Dispatcher, Handler};
use crate::http_type::Method;
use tokio::net::{TcpListener};
use tokio::task::{spawn};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use crate::http_object::{HttpRequest, HttpResponse};

pub struct ServerBuilder<'a> {
    host: Option<&'a str>,
    port: Option<u16>,
    dispatcher: Dispatcher
}


impl <'a> ServerBuilder<'a> {

    pub fn new() -> Self {
        ServerBuilder {
            host: None,
            port: None,
            dispatcher: Dispatcher::new()
        }
    }

    pub fn host(&mut self, host: &'a str) -> &mut Self {
        self.host.replace(host);
        self
    }

    pub fn port(&mut self, port: u16) -> &mut Self {
        self.port.replace(port);
        self
    }

    pub fn add<F, Fut>(&mut self, method: Method, path: &str, handler: F) -> anyhow::Result<&mut Self>
    where
        F: Fn(HttpRequest, HttpResponse) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<HttpResponse>> + Send + 'static
    {
        let closure: Handler = Arc::new(
            move |req, res|  Box::pin(handler(req, res))
        );
        self.dispatcher.add(method, path, closure)?;
        Ok(self)
    }

    pub fn build(self) -> Server {
        Server {
            host: self.host.unwrap().to_string(),
            port: self.port.unwrap(),
            dispatcher: Arc::new(self.dispatcher)
        }
    }
}



pub struct Server {
    host: String,
    port: u16,
    dispatcher: Arc<Dispatcher>
}

impl Server {

    pub async fn serve(&mut self) -> anyhow::Result<()> {

        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(addr).await?;
        let mut incoming = TcpListenerStream::new(listener);

        while let Some(item) = incoming.next().await {
            match item {
                Ok(tcp_stream) => {
                    let dis = self.dispatcher.clone();
                    // dispatch(&self, ...)를 tokio::spawn에 바로 넘기면, 생성된 Future가 스택에 있는 self를 &self로 캡처(빌림)함.
                    // spawn은 Future: Send + 'static을 요구하므로 스택에 의존하는 빌림이 있으면 거절됨.
                    // 해결은 “소유”하도록 해야한다.
                    // 즉, Arc<Dispatcher>를 값으로 async move 태스크에 옮겨 담으면 그 Future는 더 이상 외부 스택을 참조하지 않으므로 'static 요구를 만족합니다.
                    // Arc를 소유권으로 캡처하면서 빌림을 없애기 때문에 그 Future가 'static이 되는것임.
                    let _join_handle = spawn(async move {
                        dis.dispatch(tcp_stream).await});

                },
                Err(err) => {
                    println!("Failed to establish connection: {}", err);
                }
            }
        }

        Ok(())
    }


}

