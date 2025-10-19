#[derive(Debug)]
pub struct Http2ConnectionOptions {
    header_table_size: u32,
    enable_push: bool,
    max_concurrent_streams: u32,
    initial_window_size: u32,
    max_frame_size: u32,
    max_header_list_size: u32
}

#[derive(Debug)]
pub struct PartialHttp2ConnectionOptions {
    pub header_table_size: Option<u32>,
    pub enable_push: Option<bool>,
    pub max_concurrent_streams: Option<u32>,
    pub initial_window_size: Option<u32>,
    pub max_frame_size: Option<u32>,
    pub max_header_list_size: Option<u32>,
}

impl PartialHttp2ConnectionOptions {

    pub fn new() -> Self{
        PartialHttp2ConnectionOptions {
            header_table_size: None,
            enable_push: None,
            max_concurrent_streams: None,
            initial_window_size: None,
            max_frame_size: None,
            max_header_list_size: None,
        }
    }

}

impl Default for Http2ConnectionOptions {
    fn default() -> Self {
        Http2ConnectionOptions {
            header_table_size: 4096,
            enable_push: false,
            max_concurrent_streams: 100,
            initial_window_size: 65535,
            max_frame_size: 16384,
            max_header_list_size: 1
        }
    }

}

impl Http2ConnectionOptions {

    pub fn new(header_table_size: u32, enable_push: bool, max_concurrent_streams: u32,
               initial_window_size: u32, max_frame_size: u32, max_header_list_size: u32
    ) -> Self {
        Http2ConnectionOptions {
            header_table_size,
            enable_push,
            max_concurrent_streams,
            initial_window_size,
            max_frame_size,
            max_header_list_size
        }
    }

    pub fn update(&mut self, partial_options: PartialHttp2ConnectionOptions) {
        if let Some(v) = partial_options.header_table_size {
            self.header_table_size = v;
        }
        if let Some(v) = partial_options.enable_push {
            self.enable_push = v;
        }
        if let Some(v) = partial_options.max_concurrent_streams {
            self.max_concurrent_streams = v;
        }
        if let Some(v) = partial_options.initial_window_size {
            self.initial_window_size = v;
        }
        if let Some(v) = partial_options.max_frame_size {
            self.max_frame_size = v;
        }
        if let Some(v) = partial_options.max_header_list_size {
            self.max_header_list_size = v;
        }
    }

    pub fn get_max_concurrent_streams(&self) -> u32 {
        self.max_concurrent_streams
    }

    pub fn get_max_frame_size(&self) -> u32 {
        self.max_frame_size
    }

    pub fn get_header_table_size(&self) -> u32 {
        self.header_table_size
    }

    pub fn get_max_header_list_size(&self) -> u32 {
        self.max_header_list_size
    }

}