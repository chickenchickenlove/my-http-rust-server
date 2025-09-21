use std::sync::Arc;
use crate::dispatcher::{Dispatcher, Handler};

use tokio::net::{TcpListener, TcpStream};
use tokio::task::{spawn};
use tokio_util::io::read_buf;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;

pub struct ServerBuilder<'a> {
    host: Option<&'a str>,
    port: Option<u16>,
    dispatcher: Dispatcher
}


impl <'a> ServerBuilder<'a> {

    pub fn new() -> Self {
        ServerBuilder{host: None, port: None, dispatcher: Dispatcher::new()}
    }

    pub fn host(&mut self, host: &'a str) -> &mut Self {
        self.host.replace(host);
        self
    }

    pub fn port(&mut self, port: u16) -> &mut Self {
        self.port.replace(port);
        self
    }

    pub fn add(&mut self, method: crate::dispatcher::Method, path: &str, handler: Handler) -> &mut Self{
        self.dispatcher.add(method, path, handler);
        self
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

    pub fn new(host: &str, port: u16) -> Self {
        Server {
            host: host.to_string(),
            port: port,
            dispatcher: Arc::new(Dispatcher::new()),
        }
    }

    pub async fn serve(&mut self) -> anyhow::Result<()> {

        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(addr).await?;
        let mut incoming = TcpListenerStream::new(listener);

        while let Some(Ok(tcp_stream)) = incoming.next().await {
            let dis = self.dispatcher.clone();

            // dispatch(&self, ...)를 tokio::spawn에 바로 넘기면, 생성된 Future가 스택에 있는 self를 &self로 캡처(빌림)함.
            // spawn은 Future: Send + 'static을 요구하므로 스택에 의존하는 빌림이 있으면 거절됨.
            // 해결은 “소유”하도록 해야한다.
            // 즉, Arc<Dispatcher>를 값으로 async move 태스크에 옮겨 담으면 그 Future는 더 이상 외부 스택을 참조하지 않으므로 'static 요구를 만족합니다.
            // Arc를 소유권으로 캡처하면서 빌림을 없애기 때문에 그 Future가 'static이 되는것임.
            spawn(async move {
                dis.dispatch(tcp_stream).await});
        }

        Ok(())
    }


}

