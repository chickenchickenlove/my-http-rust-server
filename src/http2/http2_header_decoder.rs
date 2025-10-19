use std::collections::{HashMap, HashSet};
use bytes::Bytes;
use fluke_hpack::Decoder;
use fluke_hpack::decoder::DecoderError;
use crate::http2::http2_errors::{ErrorCode, Http2Error};


#[derive(Copy, Clone, Debug)]
enum Http2HeaderState {
    Initial,
    Pseudo,
    General,
}


pub struct Http2HeaderStateMachine {
    state: Http2HeaderState,
}
impl Http2HeaderStateMachine {

    fn new() -> Self {
        Http2HeaderStateMachine {
            state: Http2HeaderState::Initial
        }
    }

    fn maybe_next_state(&mut self, next_state: Http2HeaderState, stream_id: u32)
    -> anyhow::Result<()> {
        match (self.state, next_state) {
            (Http2HeaderState::Initial, Http2HeaderState::Initial) => {
                self.state = next_state;
                Ok(())
            }
            (Http2HeaderState::Initial, Http2HeaderState::Pseudo) => {
                self.state = next_state;
                Ok(())
            },
            (Http2HeaderState::Initial, Http2HeaderState::General) => {
                Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into())
            },
            (Http2HeaderState::Pseudo, Http2HeaderState::Initial) => {
                Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into())

            }
            (Http2HeaderState::Pseudo, Http2HeaderState::Pseudo) => {
                self.state = next_state;
                Ok(())
            },
            (Http2HeaderState::Pseudo, Http2HeaderState::General) => {
                self.state = next_state;
                Ok(())
            },
            (Http2HeaderState::General, Http2HeaderState::Initial) => {
                Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into())
            },
            (Http2HeaderState::General, Http2HeaderState::Pseudo) => {
                Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into())
            },
            (Http2HeaderState::General, Http2HeaderState::General) => {
                self.state = next_state;
                Ok(())
            },
        }


    }

}


pub struct Http2HeaderDecoder<'a> {
    decoder: Decoder<'a>
}


impl<'a> Http2HeaderDecoder<'a> {

    pub fn new(decoder: Decoder<'a>) -> Self {
        Http2HeaderDecoder {
            decoder
        }
    }

    pub fn set_max_table_size(&mut self, size: usize) {
        self.decoder.set_max_table_size(size)
    }

    pub fn set_max_allowed_table_size(&mut self, size: usize) {
        self.decoder.set_max_allowed_table_size(size)
    }

    pub fn decode_headers(&mut self, stream_id: u32, payload: Bytes)
        -> anyhow::Result<(HashMap<String, String>, HashSet<String>)>
    {
        let mut headers = HashMap::new();
        let mut trailers = HashSet::new();
        let mut state = Http2HeaderStateMachine::new();

        match self.decoder.decode(&payload) {
            Ok(decode_header_pairs) => {
                //   Header compression is stateful.  One compression context and one
                //    decompression context are used for the entire connection.  A decoding
                //    error in a header block MUST be treated as a connection error
                //    (Section 5.4.1) of type COMPRESSION_ERROR.

                let mut path = None;
                let mut scheme = None;
                let mut authority = None;
                let mut method = None;


                for (kk, vv) in decode_header_pairs.into_iter() {
                    let header_name = String::from_utf8(kk)?;
                    let header_value = String::from_utf8(vv)?;

                    println!("header_name: {}", header_name);
                    // validate
                    // 8.1.2.6.  Malformed Requests and Responses
                    //    A request or response that includes a payload body can include a
                    //    content-length header field.  A request or response is also malformed
                    //    if the value of a content-length header field does not equal the sum
                    //    of the DATA frame payload lengths that form the body.  A response
                    //    that is defined to have no payload, as described in [RFC7230],
                    //    Section 3.3.2, can have a non-zero content-length header field, even
                    //    though no content is included in DATA frames.
                    if header_name.eq_ignore_ascii_case("content-length") {
                        return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                    }

                    if let Err(e) = self.upper_case_then_throw_error(stream_id, header_name.as_str()) {
                        return Err(e);
                    }

                    //    All pseudo-header fields MUST appear in the header block before
                    //    regular header fields.  Any request or response that contains a
                    //    pseudo-header field that appears in a header block after a regular
                    //    header field MUST be treated as malformed (Section 8.1.2.6).
                    if let Err(e) = match header_name.as_str().starts_with(":") {
                        true => state.maybe_next_state(Http2HeaderState::Pseudo, stream_id),
                        false => state.maybe_next_state(Http2HeaderState::General, stream_id),
                    } {
                        return Err(e);
                    }

                    // 8.1.2.1.  Pseudo-Header Fields
                    //
                    //    While HTTP/1.x used the message start-line (see [RFC7230],
                    //    Section 3.1) to convey the target URI, the method of the request, and
                    //    the status code for the response, HTTP/2 uses special pseudo-header
                    //    fields beginning with ':' character (ASCII 0x3a) for this purpose.
                    //
                    //    Pseudo-header fields are not HTTP header fields.  Endpoints MUST NOT
                    //    generate pseudo-header fields other than those defined in this
                    //    document.
                    //    Pseudo-header fields are only valid in the context in which they are
                    //    defined.  Pseudo-header fields defined for requests MUST NOT appear
                    //    in responses; pseudo-header fields defined for responses MUST NOT
                    //    appear in requests.  Pseudo-header fields MUST NOT appear in
                    //    trailers.  Endpoints MUST treat a request or response that contains
                    //    undefined or invalid pseudo-header fields as malformed
                    //    (Section 8.1.2.6).
                    if header_name.starts_with(":") {
                        if !header_name.eq_ignore_ascii_case(":method") &&
                            !header_name.eq_ignore_ascii_case(":scheme") &&
                            !header_name.eq_ignore_ascii_case(":authority") &&
                            !header_name.eq_ignore_ascii_case(":path")
                        {
                            return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                        }

                        match header_name.as_str() {
                            ":method" if method.is_some() => {
                                // Duplicated case
                                return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into())
                            },
                            ":method" => {
                                method = Some(header_value);
                            }
                            ":scheme" if scheme.is_some() => {
                                // Duplicated case
                                return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                            }
                            ":scheme" => {
                                scheme = Some(header_value);
                            }
                            ":authority" if authority.is_some() => {
                                // Duplicated case
                                return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                            },
                            ":authority" => {
                                authority = Some(header_value);
                            }
                            ":path" if path.is_some() => {
                                // Duplicated case
                                return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                            },
                            ":path" => {
                                path = Some(header_value);
                            }
                            _ => ()
                        };
                    } else {
                        // TE header indicates that 'Client' can 'trailer' response from server.
                        if header_name.eq_ignore_ascii_case("te") && !header_value.eq_ignore_ascii_case("trailers") {
                            // 8.1.2.2 Connection-Specific Header Field
                            //    The only exception to this is the TE header field, which MAY be
                            //    present in an HTTP/2 request; when it is, it MUST NOT contain any
                            //    value other than "trailers". (Otherwise, malformed request)
                            return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                        }

                        if header_name.eq_ignore_ascii_case("connection") {
                            // 8.1.2.2. Connection-Specific Header Field
                            //    HTTP/2 does not use the Connection header field to indicate
                            //    connection-specific header fields; in this protocol, connection-
                            //    specific metadata is conveyed by other means.  An endpoint MUST NOT
                            //    generate an HTTP/2 message containing connection-specific header
                            //    fields; any message containing connection-specific header fields MUST
                            //    be treated as malformed (Section 8.1.2.6).
                            return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                        }

                        if header_name.eq_ignore_ascii_case("trailer") {
                            trailers = header_value.as_str()
                                .split(",")
                                .map(|s| s.to_string())
                                .collect::<HashSet<String>>()
                        }

                        headers.insert(header_name, header_value);
                    }
                }

                // 8.1.2.3: Request pseudo-header fields
                //    All HTTP/2 requests MUST include exactly one valid value for the
                //    ":method", ":scheme", and ":path" pseudo-header fields, unless it is
                //    a CONNECT request (Section 8.3).  An HTTP request that omits
                //    mandatory pseudo-header fields is malformed (Section 8.1.2.6).
                // -> malformed request

                //    Intermediaries that process HTTP requests or responses (i.e., any
                //    intermediary not acting as a tunnel) MUST NOT forward a malformed
                //    request or response.  Malformed requests or responses that are
                //    detected MUST be treated as a stream error (Section 5.4.2) of type
                //    PROTOCOL_ERROR.
                match &path {
                    Some(path) if path.is_empty()  => {
                        // 8.1.2.3: Request pseudo-header fields
                        //    o  The ":path" pseudo-header field includes the path and query parts
                        //       of the target URI (the "path-absolute" production and optionally a
                        //       '?' character followed by the "query" production (see Sections 3.3
                        //       and 3.4 of [RFC3986]).  A request in asterisk form includes the
                        //       value '*' for the ":path" pseudo-header field.
                        //
                        //       This pseudo-header field MUST NOT be empty for "http" or "https"
                        //       URIs; "http" or "https" URIs that do not contain a path component
                        //       MUST include a value of '/'.  The exception to this rule is an
                        //       OPTIONS request for an "http" or "https" URI that does not include
                        //       a path component; these MUST include a ":path" pseudo-header field
                        //       with a value of '*' (see [RFC7230], Section 5.3.4).
                        if scheme.is_some() && scheme.as_ref().unwrap().starts_with("http") {
                            return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                        }
                    },
                    _  => ()
                };

                match &method {
                    // 8.1.2.3: Request pseudo-header fields
                    // o  The ":method" pseudo-header field includes the HTTP method
                    //       ([RFC7231], Section 4).
                    Some(method) if
                        !method.eq_ignore_ascii_case("POST")  &&
                        !method.eq_ignore_ascii_case("PUT")  &&
                        !method.eq_ignore_ascii_case("DELETE") &&
                        !method.eq_ignore_ascii_case("HEAD") &&
                        !method.eq_ignore_ascii_case("GET") => {
                        return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into())
                    },
                    _ => ()
                };

                if scheme.is_none() || method.is_none() || path.is_none() || authority.is_none() {
                    // 8.1.2.2.  Connection-Specific Header Fields
                    return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                } else {
                    headers.insert(":path".to_string(), path.unwrap());
                    headers.insert(":authority".to_string(), authority.unwrap());
                    headers.insert(":scheme".to_string(), scheme.unwrap());
                    headers.insert(":method".to_string(), method.unwrap());
                }

                Ok((headers, trailers))
            },
            Err(DecoderError::InvalidMaxDynamicSize) => {
                Err(Http2Error::connection_error(ErrorCode::CompressionError).into())
            }
            Err(e) => {
                Err(Http2Error::connection_error(ErrorCode::CompressionError).into())
            }
        }
    }

    pub fn decode_trailers(&mut self, stream_id: u32, payload: Bytes, trailers: HashSet<String>)
                          -> anyhow::Result<(HashMap<String, String>)>
    {
        let mut headers = HashMap::new();

        match self.decoder.decode(&payload) {
            Ok(decode_header_pairs) => {
                for (kk, vv) in decode_header_pairs {
                    let header_name = String::from_utf8(kk)?;
                    let header_value = String::from_utf8(vv)?;

                    if let Err(e) = self.upper_case_then_throw_error(stream_id, header_name.as_str()) {
                        return Err(e);
                    }

                    if header_name.starts_with(":") {
                        return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                    }

                    if header_name.eq_ignore_ascii_case("connection") ||
                       header_name.eq_ignore_ascii_case("trailers")
                    {
                        // 8.1.2.2. Connection-Specific Header Field
                        //    HTTP/2 does not use the Connection header field to indicate
                        //    connection-specific header fields; in this protocol, connection-
                        //    specific metadata is conveyed by other means.  An endpoint MUST NOT
                        //    generate an HTTP/2 message containing connection-specific header
                        //    fields; any message containing connection-specific header fields MUST
                        //    be treated as malformed (Section 8.1.2.6).
                        return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
                    }

                    if trailers.contains(header_name.as_str()) {
                        headers.insert(header_name, header_value);
                    }
                }

                Ok(headers)
            },

            Err(DecoderError::InvalidMaxDynamicSize) => {
                Err(Http2Error::connection_error(ErrorCode::CompressionError).into())
            },
            Err(e) => {
                Err(Http2Error::connection_error(ErrorCode::CompressionError).into())
            },
        }
    }

    fn upper_case_then_throw_error(&self, stream_id: u32, header_name: &str) -> anyhow::Result<()> {
        // 8.1.2
        //    Just as in HTTP/1.x, header field names are strings of ASCII
        //    characters that are compared in a case-insensitive fashion.  However,
        //    header field names MUST be converted to lowercase prior to their
        //    encoding in HTTP/2.  A request or response containing uppercase
        //    header field names MUST be treated as malformed (Section 8.1.2.6).
        let has_uppercase = header_name.chars().any(|c| c.is_uppercase());
        if has_uppercase {
            return Err(Http2Error::stream_error(stream_id, ErrorCode::ProtocolError).into());
        };

        Ok(())
    }

}



