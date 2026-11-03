use reqwest::header::{self, HeaderMap, HeaderValue};
use url::{Host, Url};

pub(crate) const USER_AGENT: &str = if cfg!(target_os = "macos") {
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_16_0) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36"
} else if cfg!(target_os = "windows") {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36"
} else {
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36"
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GameHost(String);

impl GameHost {
    pub(crate) fn parse(host: &str) -> Option<Self> {
        let Host::Domain(host) = Host::parse(host).ok()? else {
            return None;
        };
        let host = host.strip_suffix('.').unwrap_or(&host);
        if !host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        }) {
            return None;
        }
        ["aq.com", "artix.com"]
            .into_iter()
            .any(|root| {
                host == root
                    || host
                        .strip_suffix(root)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            })
            .then(|| Self(host.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn identify(url: &Url, headers: &mut HeaderMap) -> bool {
    headers.remove(header::USER_AGENT);
    headers.remove("artixmode");
    let game = url.host_str().and_then(GameHost::parse).is_some();
    if game {
        headers.insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
        headers.insert("artixmode", HeaderValue::from_static("launcher"));
    }
    game
}

pub(crate) async fn dial(host: &GameHost, port: u16) -> std::io::Result<tokio::net::TcpStream> {
    tokio::net::TcpStream::connect((host.as_str(), port)).await
}
