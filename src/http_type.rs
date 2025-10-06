use std::str::FromStr;


#[derive(Debug)]
pub enum HttpProtocol {
    HTTP1,
    HTTP11,
    HTTP2,
    INVALID,
}

#[derive(Copy, Clone, Debug)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
}

// From<String> for Method, From<&str> for Method 구현대신 FromStr이 더 합리적이다.
// FromStr을 구현한 경우, String.parse()...를 호출해서 쓰는 것이 좋다.
// 일반적으로 From Trait을 구현했을 때 into()를 쓰는 것처럼 말이다.
impl FromStr for Method {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET"    => Ok(Method::GET),
            "POST"   => Ok(Method::POST),
            "DELETE" => Ok(Method::DELETE),
            "PUT"    => Ok(Method::PUT),
            _        => Err(()),
        }
    }
}