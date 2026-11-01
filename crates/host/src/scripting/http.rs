use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{
    Client, Method, StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{runtime::Handle, task::AbortHandle};
use url::Url;

use super::io::Ticket;

const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Request {
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
    deadline: f64,
    redirect: Redirect,
    max_redirects: u64,
}

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Redirect {
    Follow,
    Manual,
    Error,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Response {
    #[serde(skip)]
    pub(crate) body: Vec<u8>,
    headers: Vec<(String, String)>,
    status: u16,
    status_text: String,
    url: String,
}

#[derive(Serialize)]
pub(crate) struct Failure {
    reason: &'static str,
    url: String,
    detail: String,
}

impl Request {
    pub(crate) fn failure(&self, reason: &str) -> Value {
        json!({"ok":false,"error":{
            "reason":if reason == "session-closed" {"aborted"} else {"request"},
            "url":self.url,
            "detail":if reason == "session-closed" {"The script HTTP session is closed."} else {"Too many pending script I/O requests."}
        }})
    }
}

fn failure(reason: &'static str, url: &str, detail: impl ToString) -> Failure {
    Failure {
        reason,
        url: url.into(),
        detail: detail.to_string(),
    }
}

fn header_text(bytes: &[u8]) -> String {
    bytes.iter().copied().map(char::from).collect()
}

fn validate_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
}

pub(crate) struct Http {
    client: Client,
    runtime: Handle,
}

impl Http {
    pub(crate) fn new(runtime: Handle) -> Result<Arc<Self>, reqwest::Error> {
        Self::with_builder(runtime, Client::builder())
    }

    pub(crate) fn with_builder(
        runtime: Handle,
        builder: reqwest::ClientBuilder,
    ) -> Result<Arc<Self>, reqwest::Error> {
        Ok(Arc::new(Self {
            client: builder
                .no_proxy()
                .http1_only()
                .no_gzip()
                .no_brotli()
                .no_deflate()
                .no_zstd()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            runtime,
        }))
    }

    pub(crate) fn submit(
        &self,
        request: Request,
        body: Option<Vec<u8>>,
        ticket: Ticket,
    ) -> AbortHandle {
        let client = self.client.clone();
        self.runtime
            .spawn(async move {
                if !ticket.issue() {
                    return;
                }
                let result = async {
                    let mut url = Url::parse(&request.url)
                        .map_err(|error| failure("request", &request.url, error))?;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64()
                        * 1000.;
                    let left = (request.deadline - now).clamp(0., 2_147_483_647.);
                    if left == 0. {
                        return Err(failure("timeout", url.as_str(), "HTTP request timed out."));
                    }
                    tokio::time::timeout(
                        Duration::from_secs_f64(left / 1000.),
                        execute(&client, &request, &mut url, body),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        Err(failure("timeout", url.as_str(), "HTTP request timed out."))
                    })
                }
                .await;
                match result {
                    Ok(response) => ticket.complete_http(response),
                    Err(error) => ticket.complete(json!({"ok":false,"error":error})),
                }
            })
            .abort_handle()
    }
}

async fn execute(
    client: &Client,
    request: &Request,
    url: &mut Url,
    mut body: Option<Vec<u8>>,
) -> Result<Response, Failure> {
    if !validate_url(url) {
        return Err(failure(
            "request",
            url.as_str(),
            "Expected an absolute HTTP or HTTPS URL without embedded credentials.",
        ));
    }
    let mut method = Method::from_bytes(request.method.as_bytes())
        .map_err(|e| failure("request", url.as_str(), e))?;
    if method == Method::CONNECT {
        return Err(failure(
            "request",
            url.as_str(),
            "HTTP tunnels are not supported.",
        ));
    }
    if matches!(method, Method::GET | Method::HEAD) && body.is_some() {
        return Err(failure(
            "request",
            url.as_str(),
            "GET and HEAD requests cannot have a body.",
        ));
    }
    let mut headers = HeaderMap::new();
    for (name, value) in &request.headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| failure("request", url.as_str(), e))?;
        let bytes = value
            .chars()
            .map(|ch| u8::try_from(u32::from(ch)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| failure("request", url.as_str(), e))?;
        let value =
            HeaderValue::from_bytes(&bytes).map_err(|e| failure("request", url.as_str(), e))?;
        headers.insert(name, value);
    }
    for redirects in 0.. {
        let mut outgoing = client
            .request(method.clone(), url.clone())
            .headers(headers.clone());
        if let Some(body) = &body {
            outgoing = outgoing.body(body.clone());
        }
        let mut response = outgoing
            .send()
            .await
            .map_err(|e| failure("request", url.as_str(), e))?;
        let status = response.status();
        if status == StatusCode::SWITCHING_PROTOCOLS {
            return Err(failure(
                "request",
                url.as_str(),
                "HTTP protocol upgrades are not supported.",
            ));
        }
        if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
            && request.redirect != Redirect::Manual
            && let Some(location) = response.headers().get("location")
        {
            if request.redirect == Redirect::Error || redirects >= request.max_redirects {
                return Err(failure(
                    "redirect",
                    url.as_str(),
                    if request.redirect == Redirect::Error {
                        "HTTP redirects are disabled."
                    } else {
                        "HTTP request exceeded its redirect limit."
                    },
                ));
            }
            let next = url
                .join(&header_text(location.as_bytes()))
                .ok()
                .filter(validate_url)
                .ok_or_else(|| {
                    failure(
                        "redirect",
                        url.as_str(),
                        "HTTP response contained an invalid redirect URL.",
                    )
                })?;
            if next.origin() != url.origin() {
                for name in ["authorization", "cookie", "host", "proxy-authorization"] {
                    headers.remove(name);
                }
            }
            if (matches!(status.as_u16(), 301 | 302) && method == Method::POST)
                || (status == StatusCode::SEE_OTHER
                    && method != Method::GET
                    && method != Method::HEAD)
            {
                method = Method::GET;
                body = None;
                for name in [
                    "content-type",
                    "content-length",
                    "content-encoding",
                    "content-language",
                    "content-location",
                    "transfer-encoding",
                ] {
                    headers.remove(name);
                }
            }
            *url = next;
            continue;
        }
        if method != Method::HEAD
            && response
                .content_length()
                .is_some_and(|size| size > MAX_BYTES as u64)
        {
            return Err(failure(
                "too-large",
                url.as_str(),
                "HTTP response exceeds the 8388608 byte limit.",
            ));
        }
        let status_text = response
            .extensions()
            .get::<hyper::ext::ReasonPhrase>()
            .map(|reason| header_text(reason.as_bytes()))
            .unwrap_or_else(|| status.canonical_reason().unwrap_or("").into());
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| (name.to_string(), header_text(value.as_bytes())))
            .collect();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| failure("request", url.as_str(), e))?
        {
            if chunk.len() > MAX_BYTES - bytes.len() {
                return Err(failure(
                    "too-large",
                    url.as_str(),
                    "HTTP response exceeds the 8388608 byte limit.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(Response {
            body: bytes,
            headers,
            status: status.as_u16(),
            status_text,
            url: url.to_string(),
        });
    }
    unreachable!()
}
