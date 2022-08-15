use std::fmt::Display;

use axum::{http::StatusCode, response::IntoResponse, response::Response};

pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
pub struct SimpleError {
    msg: String,
    status: StatusCode,
}
impl SimpleError {
    pub fn new(msg: &str, status: StatusCode) -> SimpleError {
        SimpleError {
            msg: msg.to_string(),
            status,
        }
    }
    pub fn from<T, E>(result: Result<T, E>, status: StatusCode) -> Result<T, SimpleError>
    where
        E: Display,
    {
        match result {
            Ok(value) => Ok(value),
            Err(err) => Err(SimpleError::new(&err.to_string(), status)),
        }
    }
    pub fn from_msg<T, E>(
        result: Result<T, E>,
        status: StatusCode,
        msg: &str,
    ) -> Result<T, SimpleError> {
        match result {
            Ok(value) => Ok(value),
            Err(_err) => Err(SimpleError::new(msg, status)),
        }
    }
    pub fn catch<T, E>(result: Result<T, E>) -> Result<T, SimpleError>
    where
        E: Display,
    {
        SimpleError::from(result, StatusCode::INTERNAL_SERVER_ERROR)
    }
    pub fn catch_msg<T, E>(result: Result<T, E>, msg: &str) -> Result<T, SimpleError>
    where
        E: Display,
    {
        SimpleError::from_msg(result, StatusCode::INTERNAL_SERVER_ERROR, msg)
    }
    pub fn send_error<E>(err: E) -> Self
    where
        E: Display,
        E: std::error::Error + Send + Sync + 'static,
    {
        #[cfg(feature = "sentry")]
        sentry::capture_error(&err);
        SimpleError::new(&err.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
    }
}
#[macro_export(local_inner_macros)]
macro_rules! impl_simple_error {
    ($t:ty) => {
        impl From<$t> for SimpleError {
            fn from(err: $t) -> Self {
                SimpleError::send_error(err)
            }
        }
    };
}
impl From<AnyError> for SimpleError {
    fn from(err: AnyError) -> Self {
        let err = err.as_ref();
        if err.is::<SimpleError>() {
            let err = err.downcast_ref::<SimpleError>().unwrap();
            return SimpleError {
                msg: err.msg.clone(),
                status: err.status,
            };
        }
        #[cfg(feature = "sentry")]
        sentry::capture_error(err);
        SimpleError::new(&err.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
    }
}
impl std::fmt::Display for SimpleError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}
impl std::error::Error for SimpleError {}
impl_simple_error!(std::io::Error);

impl IntoResponse for SimpleError {
    fn into_response(self) -> Response {
        (self.status, self.msg).into_response()
    }
}
