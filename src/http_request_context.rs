use std::ascii::AsciiExt;
use std::collections::HashMap;
use std::marker::PhantomData;
use anyhow::{anyhow, bail};
use bytes::Bytes;
use crate::http_type::Method;

#[derive(Clone)]
pub struct Http1RequestContext {
    method: Method,
    path: String,
    version: String,
    headers: HashMap<String, String>,
    body: Option<Bytes>
}

#[derive(Clone)]
pub struct Http11RequestContext {
    method: Method,
    path: String,
    version: String,
    headers: HashMap<String, String>,
    body: Option<Bytes>
}

impl Http1RequestContext {

    pub fn new(method: Method,
               path: String,
               version: String,
               headers: HashMap<String, String>,
               body: Option<Bytes>) -> Self {
        Self { method, path, version, headers, body }
    }

    pub fn into_part(self) -> (Method, String, String, HashMap<String, String>, Option<Bytes>) {
        (self.method, self.path, self.version, self.headers, self.body)
    }

    pub fn should_close(&self) -> bool {
        if let Some(v) = self.headers.get("connection") {
            return !v.as_str().eq_ignore_ascii_case("keep-alive");
        };
        true
    }
}

pub struct RequestContextBuilder<R> {
    method: Option<Method>,
    path: Option<String>,
    version: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<Vec<u8>>,
    _marker: PhantomData<R>
}

impl <R> RequestContextBuilder<R> {

    pub fn new() -> Self {
        RequestContextBuilder {
            method: None,
            path: None,
            version: None,
            headers: None,
            body: None,
            _marker: PhantomData
        }
    }

    pub fn method(&mut self, method: Method) -> &mut Self {
        self.method.replace(method);
        self
    }

    // String 대신 Into<String>을 쓰는 이유는 좀 더 유연하게 쓰기 위함이다.
    pub fn path(&mut self, path: impl Into<String>) -> &mut Self {
        self.path = Some(path.into());
        self
    }

    pub fn version(&mut self, version: impl Into<String>) -> &mut Self {
        self.version = Some(version.into());
        self
    }

    pub fn headers(&mut self, headers: HashMap<String, String>) -> &mut Self {
        self.headers = Some(headers);
        self
    }

    pub fn body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self {
        self.body = Some(body.into());
        self
    }
}

impl <R> RequestContextBuilder<R>
where
    R: TryFrom<RequestContextBuilder<R>>
{

    pub fn build(self) -> anyhow::Result<R> {
        R::try_from(self)
            .map_err(||anyhow!("Request Context is invalid."))
    }

}

impl TryFrom<RequestContextBuilder<Http1RequestContext>> for Http1RequestContext {
    type Error = anyhow::Error;

    fn try_from(value: RequestContextBuilder<Http1RequestContext>) -> Result<Self, Self::Error> {
        // unwrap()에 의존. 파싱 실패 시 panic이 발생.
        // Result<Http1RequestContext>로 바꾸고 호출부에서 ? 처리.
        // match (self.method, self.path, self.version, self.headers) {
        //     (Some(m), Some(p), Some(v), Some(h)) =>
        //         Ok(Http1RequestContext{method: m, path: p, version: v, headers: h, body: self.body}),
        //     (_, _, _, _) => bail!("Invalid method or path or version or headers"),
        // }
        // 위 코드로 사용하면 1) 디버깅에 유연하지 않고, 2) 컴파일 에러가 발생할 수도 있음. (Self로 소유권 이동했다면)

        let method = value.method.ok_or_else(|| anyhow!("missing method."))?;
        let path = value.path.ok_or_else(|| anyhow!("missing path."))?;
        let version = value.version.ok_or_else(|| anyhow!("missing version."))?;
        let headers = value.headers.ok_or_else(|| anyhow!("missing headers."))?;

        let body = if let Some(b) = value.body {
            let body_byte: Bytes = b.into();
            Some(body_byte)
        } else {
            None
        };

        Ok(Http1RequestContext::new(method, path, version, headers, body))
    }
}


// // #[derive(Debug)]
// pub struct Http1RequestContextBuilder {
//     method: Option<Method>,
//     path: Option<String>,
//     version: Option<String>,
//     headers: Option<HashMap<String, String>>,
//     body: Option<Vec<u8>>
// }
//
//
// impl Http1RequestContextBuilder {
//     pub fn new() -> Self {
//         Http1RequestContextBuilder {
//             method: None,
//             path: None,
//             version: None,
//             headers: None,
//             body: None }
//     }
//
//     pub fn build(self) -> anyhow::Result<Http1RequestContext> {
//         // unwrap()에 의존. 파싱 실패 시 panic이 발생.
//         // Result<Http1RequestContext>로 바꾸고 호출부에서 ? 처리.
//         // match (self.method, self.path, self.version, self.headers) {
//         //     (Some(m), Some(p), Some(v), Some(h)) =>
//         //         Ok(Http1RequestContext{method: m, path: p, version: v, headers: h, body: self.body}),
//         //     (_, _, _, _) => bail!("Invalid method or path or version or headers"),
//         // }
//         // 위 코드로 사용하면 1) 디버깅에 유연하지 않고, 2) 컴파일 에러가 발생할 수도 있음. (Self로 소유권 이동했다면)
//         let method = self.method.ok_or_else(|| anyhow!("missing method."))?;
//         let path = self.path.ok_or_else(|| anyhow!("missing path."))?;
//         let version = self.version.ok_or_else(|| anyhow!("missing version."))?;
//         let headers = self.headers.ok_or_else(|| anyhow!("missing headers."))?;
//
//         let body= if let Some(b) = self.body {
//             let body_byte: Bytes = b.into();
//             Some(body_byte)
//         } else {
//             None
//         };
//
//         Ok(Http1RequestContext::new(method, path, version, headers, body))
//     }
//
//     pub fn method(&mut self, method: Method) -> &mut Self {
//         self.method.replace(method);
//         self
//     }
//
//     // String 대신 Into<String>을 쓰는 이유는 좀 더 유연하게 쓰기 위함이다.
//     pub fn path(&mut self, path: impl Into<String>) -> &mut Self {
//         self.path = Some(path.into());
//         self
//     }
//
//     pub fn version(&mut self, version: impl Into<String>) -> &mut Self {
//         self.version = Some(version.into());
//         self
//     }
//
//     pub fn headers(&mut self, headers: HashMap<String, String>) -> &mut Self {
//         self.headers = Some(headers);
//         self
//     }
//
//     pub fn body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self {
//         self.body = Some(body.into());
//         self
//     }
//
// }

impl Http11RequestContext {

    pub fn new(method: Method,
               path: String,
               version: String,
               headers: HashMap<String, String>,
               body: Option<Bytes>) -> Self {
        Self { method, path, version, headers, body }
    }

    pub fn into_part(self) -> (Method, String, String, HashMap<String, String>, Option<Bytes>) {
        (self.method, self.path, self.version, self.headers, self.body)
    }

    pub fn should_close(&self) -> bool {
        if let Some(v) = self.headers.get("connection") {
            return v.as_str().eq_ignore_ascii_case("close");
        }
        false
    }
}

// #[derive(Debug)]
pub struct Http11RequestContextBuilder {
    method: Option<Method>,
    path: Option<String>,
    version: Option<String>,
    headers: Option<HashMap<String, String>>,
    body: Option<Vec<u8>>
}


impl Http11RequestContextBuilder {
    pub fn new() -> Self {
        Http11RequestContextBuilder {
            method: None,
            path: None,
            version: None,
            headers: None,
            body: None }
    }

    pub fn build(self) -> anyhow::Result<Http11RequestContext> {
        let method = self.method.ok_or_else(|| anyhow!("missing method."))?;
        let path = self.path.ok_or_else(|| anyhow!("missing path."))?;
        let version = self.version.ok_or_else(|| anyhow!("missing version."))?;
        let headers = self.headers.ok_or_else(|| anyhow!("missing headers."))?;

        let body= if let Some(b) = self.body {
            let body_byte: Bytes = b.into();
            Some(body_byte)
        } else {
            None
        };

        Ok(Http11RequestContext::new(method, path, version, headers, body))
    }

    pub fn method(&mut self, method: Method) -> &mut Self {
        self.method.replace(method);
        self
    }

    // String 대신 Into<String>을 쓰는 이유는 좀 더 유연하게 쓰기 위함이다.
    pub fn path(&mut self, path: impl Into<String>) -> &mut Self {
        self.path = Some(path.into());
        self
    }

    pub fn version(&mut self, version: impl Into<String>) -> &mut Self {
        self.version = Some(version.into());
        self
    }

    pub fn headers(&mut self, headers: HashMap<String, String>) -> &mut Self {
        self.headers = Some(headers);
        self
    }

    pub fn body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self {
        self.body = Some(body.into());
        self
    }

}