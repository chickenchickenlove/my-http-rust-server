// https://datatracker.ietf.org/doc/html/rfc2616#section-13.5.1
// - Connection
// - Keep-Alive
// - Proxy-Authenticate
// - Proxy-Authorization
// - TE
// - Trailers
// - Transfer-Encoding
// - Upgrade


// 19.6.2 Compatibility with HTTP/1.0 Persistent Connections
//
// Some clients and servers might wish to be compatible with some
// previous implementations of persistent connections in HTTP/1.0
// clients and servers. Persistent connections in HTTP/1.0 are
// explicitly negotiated as they are not the default behavior. HTTP/1.0
// experimental implementations of persistent connections are faulty,
// and the new facilities in HTTP/1.1 are designed to rectify these
// problems. The problem was that some existing 1.0 clients may be
// sending Keep-Alive to a proxy server that doesn't understand
// Connection, which would then erroneously forward it to the next
// inbound server, which would establish the Keep-Alive connection and
// result in a hung HTTP/1.0 proxy waiting for the close on the
// response. The result is that HTTP/1.0 clients must be prevented from
// using Keep-Alive when talking to proxies.
//
// However, talking to proxies is the most important use of persistent
// connections, so that prohibition is clearly unacceptable. Therefore,
// we need some other mechanism for indicating a persistent connection
// is desired, which is safe to use even when talking to an old proxy
// that ignores Connection. Persistent connections are the default for
// HTTP/1.1 messages; we introduce a new keyword (Connection: close) for
// declaring non-persistence. See section 14.10.
//
// The original HTTP/1.0 form of persistent connections (the Connection:
// Keep-Alive and Keep-Alive header) is documented in RFC 2068. [33]