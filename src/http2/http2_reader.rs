use std::sync::Arc;
use fluke_hpack::Decoder;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedReadHalf;
use tokio::select;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use crate::connection_reader::Http2Handler;
use crate::http2::http2_stream::{Http2ChannelMessage, ReaderChannelMessage};

pub struct ReaderTask {
    reader: BufReader<OwnedReadHalf>,
    sender_to_connection: Sender<ReaderChannelMessage>,
    token: CancellationToken,
}

impl<'a> ReaderTask {

    pub fn with(reader: BufReader<OwnedReadHalf>,
                sender_to_connection: Sender<ReaderChannelMessage>,
                token: CancellationToken
    ) -> ReaderTask {
        ReaderTask { reader, sender_to_connection, token }
    }

    pub async fn serve(mut self) {
        let mut handler = Http2Handler::new();
        loop {
            select! {
                // Task 1 -> Handle Frame.
                maybe = handler.parse_frame(&mut self.reader) => {
                    // 분리한 이유 : 이걸 Network Connecto의 메인 루프의 select!에서 돌리게 되면,
                    // 다른 Future가 먼저 완료되었을 때, TCP 버퍼에서 읽고 있던 메세지가 드랍된다.
                    // 즉, 메세지가 유실되기 때문에 다른 프로세스로 분리했다.
                    if let Ok(Some(frame)) = maybe {
                        debug!("reader try to send a frame to connection, frame is {:?}", frame);
                        self.sender_to_connection
                            .send(
                                ReaderChannelMessage::MaybeFrameParsed { frame }
                            )
                            .await
                            .unwrap();
                    }
                    else if let Err(_) = maybe {
                        break;
                    }
                }
                _ = self.token.cancelled() => {
                    debug!("reader task is cancelled.");
                    break;
                }
            }
        }
        debug!("reader task is terminated.");
    }
}