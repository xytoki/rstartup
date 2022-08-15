use axum::{
    async_trait,
    extract::{ConnectInfo, FromRequest, RequestParts},
    headers::HeaderName,
    Extension,
};
use std::{str::FromStr};

use crate::listener::IpConnectInfo;

#[derive(Clone, Debug)]
pub struct RealIP(pub String);
#[async_trait]
impl<B> FromRequest<B> for RealIP
where
    B: Send,
{
    type Rejection = <Extension<Self> as FromRequest<B>>::Rejection;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let Extension(connect_info) =
            Extension::<ConnectInfo<IpConnectInfo>>::from_request(req).await?;
        let ip = req
            .headers()
            .get(HeaderName::from_str("x-real-ip").unwrap())
            .and_then(|header| header.to_str().ok())
            .unwrap_or(&connect_info.0.ip)
            .to_string();
        Ok(Self(ip))
    }
}
