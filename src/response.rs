use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use hyper::HeaderMap;
use std::ops::Deref;

pub type SimpleResponse<T> = (StatusCode, T);
pub type SimpleJson<T> = SimpleResponse<Json<T>>;
pub type HeaderResponse<T> = (StatusCode, HeaderMap, T);
pub type HeaderJson<T> = HeaderResponse<Json<T>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleStatus(StatusCode);

impl SimpleStatus {
    pub fn new(status: StatusCode) -> SimpleStatus {
        SimpleStatus(status)
    }
}
impl IntoResponse for SimpleStatus {
    fn into_response(self) -> Response {
        (self.0, "").into_response()
    }
}
impl Deref for SimpleStatus {
    type Target = StatusCode;
    fn deref(&self) -> &StatusCode {
        &self.0
    }
}
impl From<StatusCode> for SimpleStatus {
    fn from(status: StatusCode) -> SimpleStatus {
        SimpleStatus::new(status)
    }
}
impl From<SimpleStatus> for StatusCode {
    fn from(status: SimpleStatus) -> StatusCode {
        status.0
    }
}

#[macro_export(local_inner_macros)]
macro_rules! impl_hit_and_304 {
    ($t:ty) => {
        impl axum::response::IntoResponse for $t {
            fn into_response(self) -> axum::response::Response {
                let mut res = (StatusCode::NOT_MODIFIED, "").into_response();
                if !self._304 {
                    res = Json(self.data).into_response();
                    res.headers_mut().append(
                        axum::http::header::LAST_MODIFIED,
                        self.last_modified.parse().unwrap(),
                    );
                    res.headers_mut().append(
                        axum::http::header::CACHE_CONTROL,
                        "no-cache, max-age=600, must-revalidate".parse().unwrap(),
                    );
                }
                res.headers_mut().append(
                    <axum::headers::HeaderName as std::str::FromStr>::from_str("x-cache-lookup")
                        .unwrap(),
                    if self._hit {
                        "HIT".parse().unwrap()
                    } else {
                        "MISS".parse().unwrap()
                    },
                );
                res
            }
        }
    };
}
