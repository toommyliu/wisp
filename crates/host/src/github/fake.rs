use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::sync::oneshot;
use url::Url;

use super::{BoxFuture, Clock, HttpError, Response, Transport};

pub(crate) enum Step {
    Respond(Result<Response, HttpError>),
    Delay(Duration, Response),
    Gate(oneshot::Receiver<Result<Response, HttpError>>),
    Hang,
}

pub(crate) struct Seen {
    pub(crate) url: Url,
    pub(crate) headers: HeaderMap,
}

#[derive(Default)]
pub(crate) struct Fake {
    steps: Mutex<VecDeque<Step>>,
    seen: Mutex<Vec<Seen>>,
    dropped: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    pub(crate) fn new(steps: impl IntoIterator<Item = Step>) -> Arc<Self> {
        let fake = Arc::new(Self::default());
        fake.steps.lock().extend(steps);
        fake
    }

    pub(crate) fn push(&self, step: Step) {
        self.steps.lock().push_back(step);
    }

    pub(crate) fn clear(&self) {
        self.steps.lock().clear();
    }

    pub(crate) fn requests(&self) -> usize {
        self.seen.lock().len()
    }

    pub(crate) fn urls(&self) -> Vec<String> {
        self.seen
            .lock()
            .iter()
            .map(|seen| seen.url.to_string())
            .collect()
    }

    pub(crate) fn headers(&self) -> Vec<HeaderMap> {
        self.seen
            .lock()
            .iter()
            .map(|seen| seen.headers.clone())
            .collect()
    }

    pub(crate) fn dropped(&self) -> Vec<String> {
        self.dropped.lock().clone()
    }

    pub(crate) async fn reached(&self, requests: usize) {
        while self.requests() < requests {
            tokio::task::yield_now().await;
        }
    }
}

struct Pending {
    url: String,
    dropped: Arc<Mutex<Vec<String>>>,
    done: bool,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.done {
            self.dropped.lock().push(std::mem::take(&mut self.url));
        }
    }
}

impl Transport for Fake {
    fn get(
        &self,
        url: Url,
        headers: HeaderMap,
        max_bytes: usize,
        _timeout: Duration,
    ) -> BoxFuture<Result<Response, HttpError>> {
        let step = self.steps.lock().pop_front();
        let pending = Pending {
            url: url.to_string(),
            dropped: self.dropped.clone(),
            done: false,
        };
        self.seen.lock().push(Seen { url, headers });
        Box::pin(async move {
            let mut pending = pending;
            let outcome = match step {
                None => Err(HttpError::Failed),
                Some(Step::Respond(outcome)) => outcome,
                Some(Step::Delay(delay, response)) => {
                    tokio::time::sleep(delay).await;
                    Ok(response)
                }
                Some(Step::Gate(gate)) => gate.await.unwrap_or(Err(HttpError::Failed)),
                Some(Step::Hang) => std::future::pending().await,
            };
            pending.done = true;
            match outcome {
                Ok(response) if response.body.len() > max_bytes => {
                    Err(HttpError::TooLarge(max_bytes))
                }
                outcome => outcome,
            }
        })
    }
}

pub(crate) fn response(status: u16, headers: &[(&str, &str)], body: &str) -> Response {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        map.append(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    Response {
        status: StatusCode::from_u16(status).unwrap(),
        headers: map,
        body: body.as_bytes().to_vec(),
    }
}

pub(crate) fn clock(start: SystemTime) -> Clock {
    let origin = tokio::time::Instant::now();
    Arc::new(move || start + origin.elapsed())
}
