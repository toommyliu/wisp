mod http;

use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

use crate::net::GameHost;

pub(crate) struct Timeouts {
    pub connect: Duration,
    pub read: Duration,
    pub request: Duration,
}

const TIMEOUTS: Timeouts = Timeouts {
    connect: Duration::from_secs(30),
    read: Duration::from_secs(60),
    request: Duration::from_secs(300),
};

pub(crate) struct Net {
    rt: tokio::runtime::Handle,
    http: reqwest::Client,
    cookies: std::sync::Arc<reqwest::cookie::Jar>,
    deadline: Duration,
    tasks: Mutex<Vec<tokio::task::AbortHandle>>,
    #[cfg(test)]
    routes: Vec<(&'static str, std::net::SocketAddr)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
}

pub(crate) struct HttpRequest {
    pub method: Method,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<(Vec<u8>, String)>,
}

pub(crate) struct HttpResponse {
    pub url: Url,
    pub status: u16,
    pub content_type: Option<String>,
    pub length: Option<u64>,
    pub body: HttpBody,
}

#[derive(Clone)]
pub(crate) struct HttpBody {
    chunks: async_channel::Receiver<Vec<u8>>,
    failure: Arc<OnceLock<NetError>>,
}

impl HttpBody {
    pub(crate) async fn next_chunk(&self) -> Result<Option<Vec<u8>>, NetError> {
        match self.chunks.recv().await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(_) => match self.failure.get() {
                Some(error) => Err(error.clone()),
                None => Ok(None),
            },
        }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum NetError {
    #[error("{0}")]
    Http(String),
    #[error("the request timed out")]
    TimedOut,
    #[error("the net runtime is gone")]
    Shutdown,
}

impl From<reqwest::Error> for NetError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::TimedOut
        } else {
            Self::Http(error.to_string())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SocketEvent {
    Connected,
    Failed,
    TimedOut,
    Data(Vec<u8>),
    Closed,
}

impl Net {
    pub(crate) fn new(rt: tokio::runtime::Handle) -> Result<Net, NetError> {
        Self::build(rt, TIMEOUTS, reqwest::Client::builder())
    }

    #[cfg(test)]
    pub(crate) fn with_timeouts(
        rt: tokio::runtime::Handle,
        timeouts: Timeouts,
    ) -> Result<Net, NetError> {
        Self::build(rt, timeouts, reqwest::Client::builder().no_proxy())
    }

    fn build(
        rt: tokio::runtime::Handle,
        timeouts: Timeouts,
        builder: reqwest::ClientBuilder,
    ) -> Result<Net, NetError> {
        let http = builder
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeouts.connect)
            .read_timeout(timeouts.read)
            .build()?;
        Ok(Net {
            rt,
            http,
            cookies: Default::default(),
            deadline: timeouts.request,
            tasks: Mutex::new(Vec::new()),
            #[cfg(test)]
            routes: Vec::new(),
        })
    }

    #[cfg(test)]
    fn for_test(
        rt: tokio::runtime::Handle,
        routes: Vec<(&'static str, std::net::SocketAddr)>,
    ) -> Net {
        let mut builder = reqwest::Client::builder().no_proxy();
        for (host, addr) in &routes {
            builder = builder.resolve(host, *addr);
        }
        let mut net = Self::build(rt, TIMEOUTS, builder).unwrap();
        net.routes = routes;
        net
    }

    pub(crate) fn connect(
        &self,
        host: String,
        port: u16,
        timeout: Duration,
        outgoing: async_channel::Receiver<Vec<u8>>,
        mut deliver: impl FnMut(SocketEvent) -> bool + Send + 'static,
    ) {
        let Some(host) = GameHost::parse(&host) else {
            tracing::warn!(%host, port, "socket host rejected");
            deliver(SocketEvent::Failed);
            return;
        };
        #[cfg(test)]
        let address = self
            .routes
            .iter()
            .find(|(name, _)| *name == host.as_str())
            .map(|(_, addr)| *addr);
        self.track(self.rt.spawn(async move {
            let connecting = async {
                #[cfg(test)]
                if let Some(address) = address {
                    return tokio::net::TcpStream::connect(address).await;
                }
                crate::net::dial(&host, port).await
            };
            let stream = match tokio::time::timeout(timeout, connecting).await {
                Ok(Ok(stream)) => stream,
                Ok(Err(error)) => {
                    tracing::warn!(host = %host.as_str(), port, %error, "socket connection failed");
                    deliver(SocketEvent::Failed);
                    return;
                }
                Err(_) => {
                    deliver(SocketEvent::TimedOut);
                    return;
                }
            };
            if !deliver(SocketEvent::Connected) {
                return;
            }
            let (mut read, mut write) = stream.into_split();
            let reading = async {
                let mut bytes = [0; 4096];
                loop {
                    match read.read(&mut bytes).await {
                        Ok(0) | Err(_) => break,
                        Ok(count) => {
                            if !deliver(SocketEvent::Data(bytes[..count].to_vec())) {
                                break;
                            }
                        }
                    }
                }
            };
            let writing = async {
                while let Ok(bytes) = outgoing.recv().await {
                    if write.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                let _ = write.shutdown().await;
            };
            tokio::select! { _ = reading => {}, _ = writing => {} }
            deliver(SocketEvent::Closed);
        }));
    }

    fn track(&self, task: tokio::task::JoinHandle<()>) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|task| !task.is_finished());
        tasks.push(task.abort_handle());
    }
}

impl Drop for Net {
    fn drop(&mut self) {
        for task in self.tasks.get_mut().unwrap().drain(..) {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    fn accept(listener: &TcpListener) -> TcpStream {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    return stream;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("expected a socket connection: {error}"),
            }
        }
    }

    #[test]
    fn socket_policy_uses_canonical_dns_label_boundaries() {
        for host in [
            "aq.com",
            "sock7.aq.com",
            "SOCK8.AQ.COM.",
            "euro.aqw.artix.com",
            "asia.game.artix.com",
        ] {
            assert!(GameHost::parse(host).is_some(), "{host}");
        }
        for host in [
            "127.0.0.1",
            "2130706433",
            "[::1]",
            "::1",
            "evil-aq.com",
            "aq.com.evil",
            "artix.com.evil",
            "aq.com:5588",
            "aq.com/path",
            "user@aq.com",
            "aq.com..",
            "",
        ] {
            assert!(GameHost::parse(host).is_none(), "{host}");
        }
    }

    #[test]
    fn forbidden_socket_hostnames_are_rejected_before_dialing() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let net = Net::for_test(runtime.handle().clone(), vec![("aq.com.evil", addr)]);
        let (_tx, rx) = async_channel::unbounded();
        let (events, received) = mpsc::channel();
        net.connect(
            "aq.com.evil".into(),
            addr.port(),
            Duration::from_secs(1),
            rx,
            move |event| events.send(event).is_ok(),
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(1)).unwrap(),
            SocketEvent::Failed
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn allowed_sockets_forward_arbitrary_bytes_unchanged() {
        const OUTBOUND: &[u8] = b"\xffclient\0unfinished";
        const INBOUND: &[u8] = b"\x80server\0partial";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut stream = accept(&listener);
            let mut bytes = vec![0; OUTBOUND.len()];
            stream.read_exact(&mut bytes).unwrap();
            assert_eq!(bytes, OUTBOUND);
            for chunk in INBOUND.chunks(3) {
                stream.write_all(chunk).unwrap();
            }
        });
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let net = Net::for_test(runtime.handle().clone(), vec![("sock7.aq.com", addr)]);
        let (tx, rx) = async_channel::unbounded();
        let (events, received) = mpsc::channel();
        net.connect(
            "sock7.aq.com".into(),
            addr.port(),
            Duration::from_secs(5),
            rx,
            move |event| events.send(event).is_ok(),
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            SocketEvent::Connected
        );
        tx.send_blocking(OUTBOUND[..3].to_vec()).unwrap();
        tx.send_blocking(OUTBOUND[3..].to_vec()).unwrap();
        let mut bytes = Vec::new();
        loop {
            match received
                .recv_timeout(Duration::from_secs(5))
                .expect("the socket never reported Closed")
            {
                SocketEvent::Data(chunk) => bytes.extend(chunk),
                SocketEvent::Closed => break,
                other => panic!("unexpected socket event {other:?}"),
            }
        }
        assert_eq!(bytes, INBOUND);
        server.join().unwrap();
    }

    #[test]
    fn dropping_the_backend_closes_its_sockets() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut stream = accept(&listener);
            assert_eq!(stream.read(&mut [0]).unwrap(), 0);
        });
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let net = Net::for_test(runtime.handle().clone(), vec![("sock7.aq.com", addr)]);
        let (_tx, rx) = async_channel::unbounded();
        let (events, received) = mpsc::channel();
        net.connect(
            "sock7.aq.com".into(),
            addr.port(),
            Duration::from_secs(5),
            rx,
            move |event| events.send(event).is_ok(),
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            SocketEvent::Connected
        );
        drop(net);
        server.join().unwrap();
    }
}
