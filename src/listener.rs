use axum::{extract::connect_info, Router};
use hyper::server::conn::AddrStream;
use listenfd::ListenFd;
use std::{net::SocketAddr, str::FromStr};
use tokio::signal;

#[cfg(unix)]
use hyperlocal::UnixServerExt;

pub async fn listen<F>(addr: &str, app: F) -> anyhow::Result<()>
where
    F: FnOnce(&str) -> Router,
{
    if addr.starts_with("fd:") {
        let mut listenfd = ListenFd::from_env();
        let listener = listenfd.take_tcp_listener(0);
        if listener.is_err() {
            tracing::error!("listenfd faild: {}", listener.unwrap_err().to_string());
            std::process::exit(2101);
        }
        let listener = listener.unwrap();
        if listener.is_none() {
            tracing::error!("listenfd faild: no listener");
            std::process::exit(2102);
        }
        let s = axum::Server::from_tcp(listener.unwrap());
        let app = app("fd:tcp");
        let server = s
            .unwrap()
            .serve(app.into_make_service_with_connect_info::<IpConnectInfo>())
            .with_graceful_shutdown(shutdown_signal());
        if let Err(e) = server.await {
            tracing::error!("server faild to start: {}", e);
            std::process::exit(3);
        }
    } else if addr.starts_with("fd+unix:") {
        #[cfg(not(unix))]
        {
            tracing::error!("unix socket is not supported on this platform");
            std::process::exit(9);
        }
        #[cfg(unix)]
        {
            let mut listenfd = ListenFd::from_env();
            let listener = listenfd.take_unix_listener(0);
            if listener.is_err() {
                tracing::error!("listenfd faild: {}", listener.unwrap_err().to_string());
                std::process::exit(2101);
            }
            let listener = listener.unwrap();
            if listener.is_none() {
                tracing::error!("listenfd faild: no listener");
                std::process::exit(2102);
            }
            let listener = listener.unwrap();
            listener.set_nonblocking(true).expect("Couldn't set non blocking");
            let s = axum::Server::builder(hyperlocal::SocketIncoming::from_listener(
                tokio::net::UnixListener::from_std(listener).unwrap(),
            ));
            let app = app("fd:unix");
            let server = s
                .serve(app.into_make_service_with_connect_info::<IpConnectInfo>())
                .with_graceful_shutdown(shutdown_signal());
            if let Err(e) = server.await {
                tracing::error!("server faild to start: {}", e);
                std::process::exit(3);
            }
        }
    } else if addr.starts_with("unix:") {
        #[cfg(not(unix))]
        {
            tracing::error!("unix socket is not supported on this platform");
            std::process::exit(9);
        }
        #[cfg(unix)]
        {
            let path = std::path::Path::new(addr.strip_prefix("unix:").unwrap());
            if path.exists() {
                std::fs::remove_file(path).unwrap_or(());
            }
            let s = axum::Server::bind_unix(path);
            if s.is_err() {
                tracing::error!("unable to bind to {}", addr);
                std::process::exit(2201);
            }
            let app = app(addr);
            let server = s
                .unwrap()
                .serve(app.into_make_service_with_connect_info::<IpConnectInfo>())
                .with_graceful_shutdown(shutdown_signal());
            if let Err(e) = server.await {
                tracing::error!("server faild to start: {}", e);
                std::process::exit(3);
            }
        }
    } else {
        let s = SocketAddr::from_str(addr).unwrap();
        let s = axum::Server::try_bind(&s);
        if s.is_err() {
            tracing::error!("unable to bind to {}", addr);
            std::process::exit(2301);
        }
        let app = app(addr);
        let server = s
            .unwrap()
            .serve(app.into_make_service_with_connect_info::<IpConnectInfo>())
            .with_graceful_shutdown(shutdown_signal());
        if let Err(e) = server.await {
            tracing::error!("server faild to start: {}", e);
            std::process::exit(3);
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct IpConnectInfo {
    pub ip: String,
    pub port: u16,
}
impl std::fmt::Display for IpConnectInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}
impl connect_info::Connected<&AddrStream> for IpConnectInfo {
    fn connect_info(target: &AddrStream) -> Self {
        let ip = target.remote_addr().ip().to_string();
        let port = target.remote_addr().port();
        Self { ip, port }
    }
}

#[cfg(unix)]
impl connect_info::Connected<&tokio::net::UnixStream> for IpConnectInfo {
    fn connect_info(_target: &tokio::net::UnixStream) -> Self {
        Self {
            ip: "127.0.0.0".to_string(),
            port: 0,
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        tracing::info!("Ctrl+C received, exiting...");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
        tracing::info!("SIGTERM received, exiting...");
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
