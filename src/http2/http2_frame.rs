
// #[EnumRepr(type = "u16")]
#[derive(Debug, Clone, Copy)]
pub enum SettingIdentifier {
    HeaderTableSize = 0x01,
    EnablePush = 0x02,
    MaxConcurrentStreams = 0x03,
    InitialWindowSize = 0x04,
    MaxFrameSize = 0x05,
    MaxHeaderListSize = 0x06,
}


