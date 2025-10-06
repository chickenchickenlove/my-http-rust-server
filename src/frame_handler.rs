use bytes::Bytes;
use fluke_h2_parse::{
    preface,
    Frame,
    FrameType,
    FrameType::Settings,
    SettingsFlags,
    StreamId
};
use fluke_h2_parse::enumflags2::BitFlags;
use fluke_hpack::{
    Decoder,
    Encoder
};

pub struct FrameHandle {

}

impl FrameHandle {

    pub fn new() -> Self {
        FrameHandle {}
    }

    pub async fn handle(&self, frame: &Frame) -> anyhow::Result<()>{
        match frame.frame_type {
            Settings(_) => {
                SettingsFrameHandler::new()
                    .handle(frame)
                    .await
            },
            _ => {
                println!("hello2");
                Ok(())
            }
        }
    }
}



trait FrameHandler {

    async fn handle(&self, frame: &Frame) -> anyhow::Result<()> {
        self.validate(&frame).await?;
        self.process(&frame).await?;
        self.maybe_ack(&frame).await?;
        Ok(())
    }
    async fn validate(&self, frame: &Frame) -> anyhow::Result<()>;
    async fn maybe_ack(&self, frame: &Frame) -> anyhow::Result<()>;
    async fn process(&self, frame: &Frame) -> anyhow::Result<()>;

}


struct SettingsFrameHandler { }

impl SettingsFrameHandler {
    pub fn new() -> Self {
        SettingsFrameHandler {}
    }
}


impl FrameHandler for SettingsFrameHandler {

    // RFC : https://datatracker.ietf.org/doc/html/rfc7540#section-6.5

    async fn validate(&self, frame: &Frame) -> anyhow::Result<()>{
        Ok(())
    }

    async fn maybe_ack(&self, frame: &Frame) -> anyhow::Result<()> {
        let ack_frame = Frame::new(Settings(BitFlags::from(SettingsFlags::Ack)), StreamId(0));
        let hello = h2_frame_to_bytes(ack_frame, &[]);
        Ok(())
    }

    async fn process(&self, frame: &Frame) -> anyhow::Result<()> {
        Ok(())
    }
}




// 공용 헬퍼: 9바이트 헤더 + payload 쓰기
// TODO : 나중에 공부하기.
fn h2_frame_to_bytes(
    frame: Frame,
    // frame_type: u8,   // SETTINGS=0x04
    // flags: u8,        // ACK=0x01
    // stream_id: u32,   // SETTINGS는 0
    payload: &[u8],   // ACK는 빈 페이로드
) -> anyhow::Result<Bytes> {



    let frame_type_byte = match frame.frame_type {
        Settings(_) => 0x04,
        _ => 0x01
    };

    let flags = match frame.frame_type {
        Settings(flags) => flags.bits(),
        _ => 0x01,
    };

    let sid  = frame.stream_id.0 & 0x7FFF_FFFF;
    let len = payload.len();
    let mut hdr = [0u8; 9];

    // length (24-bit, big-endian)
    hdr[0] = ((len >> 16) & 0xFF) as u8;
    hdr[1] = ((len >> 8)  & 0xFF) as u8;
    hdr[2] = ( len        & 0xFF) as u8;

    hdr[3] = frame_type_byte;// type
    hdr[4] = flags;          // flags

    // let sid = stream_id & 0x7FFF_FFFF; // R 비트 0
    hdr[5] = ((sid >> 24) & 0xFF) as u8;
    hdr[6] = ((sid >> 16) & 0xFF) as u8;
    hdr[7] = ((sid >> 8)  & 0xFF) as u8;
    hdr[8] = ( sid        & 0xFF) as u8;

    Ok(Bytes::copy_from_slice(&hdr))
}
