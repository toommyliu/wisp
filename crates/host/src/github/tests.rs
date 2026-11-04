use super::fake::{Fake, Step, clock, response};
use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;
use tokio::sync::oneshot;

const API_URL: &str = "https://api.github.com/repos/example/tools";

fn github(fake: &Arc<Fake>) -> Arc<GitHub> {
    let start = UNIX_EPOCH + Duration::from_secs(1_785_542_400);
    Arc::new(GitHub::new(
        fake.clone(),
        "Lucent/test",
        clock(start),
        || 0.5,
    ))
}

fn get(attempts: u32) -> Get {
    Get {
        url: Url::parse(API_URL).unwrap(),
        etag: None,
        attempts,
        max_bytes: 1024,
        timeout: Duration::from_secs(10),
    }
}

fn secondary_limit(headers: &[(&str, &str)]) -> Step {
    Step::Respond(Ok(response(
        403,
        headers,
        "You have exceeded a secondary rate limit.",
    )))
}

fn ok() -> Step {
    Step::Respond(Ok(response(200, &[], "")))
}

fn gate() -> (Step, oneshot::Sender<Result<Response, HttpError>>) {
    let (send, receive) = oneshot::channel();
    (Step::Gate(receive), send)
}

fn cooldown_ms(error: GitHubError, started: DateTime<Utc>) -> i64 {
    match error {
        GitHubError::RateLimited(until) => (until - started).num_milliseconds(),
        other => panic!("expected a rate limit, got {other:?}"),
    }
}

async fn fails(github: &GitHub) -> GitHubError {
    github.get(&get(1)).await.err().expect("the request fails")
}

struct Local {
    address: SocketAddr,
    requests: AtomicUsize,
}

impl Transport for Local {
    fn get(
        &self,
        _url: Url,
        headers: HeaderMap,
        max_bytes: usize,
        timeout: Duration,
    ) -> BoxFuture<Result<Response, HttpError>> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let url = Url::parse(&format!("http://{}/", self.address)).unwrap();
        Box::pin(async move {
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .unwrap();
            request(&client, url, headers, max_bytes, timeout).await
        })
    }
}

#[tokio::test]
async fn rejects_a_repository_redirect_without_retrying() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(
                b"HTTP/1.1 301 Moved Permanently\r\nLocation: https://api.github.com/repos/example/renamed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let local = Arc::new(Local {
        address,
        requests: AtomicUsize::new(0),
    });
    let github = GitHub::new(
        local.clone(),
        "Lucent/test",
        Arc::new(SystemTime::now),
        || 0.5,
    );

    let error = github.get(&get(3)).await.err().unwrap();

    assert_eq!(error, GitHubError::Http(HttpError::Redirect));
    assert_eq!(local.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn live_transport_refuses_bodies_past_the_limit_by_length_and_by_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let replies: [&[u8]; 2] = [
            b"HTTP/1.1 200 OK\r\nContent-Length: 2048\r\nConnection: close\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n400\r\n",
        ];
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(reply);
            if reply.ends_with(b"400\r\n") {
                let _ = stream.write_all(&[b'x'; 1024]);
                let _ = stream.write_all(b"\r\n400\r\n");
                let _ = stream.write_all(&[b'x'; 1024]);
                let _ = stream.write_all(b"\r\n0\r\n\r\n");
            }
        }
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = Url::parse(&format!("http://{address}/")).unwrap();
    for _ in 0..2 {
        let outcome = request(
            &client,
            url.clone(),
            HeaderMap::new(),
            1500,
            Duration::from_secs(10),
        )
        .await;
        assert_eq!(outcome.err(), Some(HttpError::TooLarge(1500)));
    }
}

#[tokio::test(start_paused = true)]
async fn classifies_and_retries_transport_failures() {
    let cases = [
        (HttpError::Redirect, 1),
        (HttpError::TooLarge(1024), 1),
        (HttpError::Failed, 3),
        (HttpError::TimedOut, 3),
    ];
    for (failure, requests) in cases {
        let fake = Fake::new((0..3).map(|_| Step::Respond(Err(failure.clone()))));
        let error = github(&fake).get(&get(3)).await.err().unwrap();
        assert_eq!(
            (error, fake.requests()),
            (GitHubError::Http(failure), requests)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn pins_retry_timestamps_and_cooldown_admission_to_the_clock() {
    let fake = Fake::new([
        Step::Respond(Ok(response(403, &[("retry-after", "30")], ""))),
        ok(),
    ]);
    let github = Arc::new(GitHub::new(
        fake.clone(),
        "Lucent/test",
        clock(UNIX_EPOCH + Duration::from_millis(1_234_567_890_000)),
        || 0.5,
    ));

    let limited = fails(&github).await;
    assert_eq!(
        limited.to_string(),
        "GitHub's request limit has been reached. Try again after 2009-02-13T23:32:00.000Z."
    );
    tokio::time::advance(Duration::from_millis(29_999)).await;
    assert_eq!(fails(&github).await, limited);
    assert_eq!(fake.requests(), 1);
    tokio::time::advance(Duration::from_millis(1)).await;
    assert_eq!(github.get(&get(1)).await.unwrap().status, StatusCode::OK);
    assert_eq!(fake.requests(), 2);
}

#[test]
fn derives_cooldowns_from_retry_after_and_primary_reset_headers() {
    let now = DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
        .unwrap()
        .to_utc();
    let headers = |pairs: &[(&str, &str)]| response(200, pairs, "").headers;
    let after = |seconds| now + TimeDelta::seconds(seconds);

    assert_eq!(
        retry_at(&headers(&[("retry-after", "30")]), now),
        Some(after(30))
    );
    let reset = (now.timestamp() + 45).to_string();
    assert_eq!(
        retry_at(
            &headers(&[
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset", &reset)
            ]),
            now
        ),
        Some(after(45))
    );
    assert_eq!(
        retry_at(
            &headers(&[("retry-after", "Sat, 01 Aug 2026 00:01:00 GMT")]),
            now
        ),
        Some(after(60))
    );
    assert_eq!(retry_at(&headers(&[("retry-after", "-5")]), now), Some(now));
    assert_eq!(retry_at(&headers(&[("retry-after", "1e300")]), now), None);
    assert_eq!(
        retry_at(
            &headers(&[
                ("x-ratelimit-remaining", "1"),
                ("x-ratelimit-reset", &reset)
            ]),
            now
        ),
        None
    );
}

#[test]
fn distinguishes_rate_limits_from_ordinary_permission_failures() {
    let now = Utc::now();
    assert!(is_rate_limit(
        &response(403, &[], "secondary rate limit"),
        now
    ));
    assert!(is_rate_limit(
        &response(403, &[], "API RATE LIMIT exceeded"),
        now
    ));
    assert!(!is_rate_limit(
        &response(403, &[], "organization policy denied access"),
        now
    ));
    assert!(is_rate_limit(&response(429, &[], ""), now));
    assert!(!is_rate_limit(&response(404, &[], "rate limit"), now));
}

#[tokio::test(start_paused = true)]
async fn backs_off_and_stops_after_three_attempts() {
    for last in [200, 503] {
        let fake = Fake::new([
            Step::Respond(Ok(response(503, &[], ""))),
            Step::Respond(Ok(response(502, &[], ""))),
            Step::Respond(Ok(response(last, &[], ""))),
        ]);
        let github = github(&fake);
        let task = tokio::spawn({
            let github = github.clone();
            async move { github.get(&get(3)).await.map(|response| response.status) }
        });
        let pass = |ms| async move {
            tokio::time::advance(Duration::from_millis(ms)).await;
            tokio::task::yield_now().await;
        };
        fake.reached(1).await;

        pass(199).await;
        assert_eq!(fake.requests(), 1);
        pass(1).await;
        assert_eq!(fake.requests(), 2);
        pass(399).await;
        assert_eq!(fake.requests(), 2);
        assert!(!task.is_finished());
        pass(1).await;
        let outcome = task.await.unwrap();
        if last == 200 {
            assert_eq!(outcome, Ok(StatusCode::OK));
        } else {
            assert_eq!(
                outcome,
                Err(GitHubError::Unavailable(StatusCode::SERVICE_UNAVAILABLE))
            );
        }
        pass(10_000).await;
        assert_eq!(fake.requests(), 3);
    }
}

#[tokio::test(start_paused = true)]
async fn fails_permission_errors_without_retrying() {
    let fake = Fake::new([Step::Respond(Ok(response(
        403,
        &[],
        "organization policy denied",
    )))]);

    let error = github(&fake).get(&get(3)).await.err().unwrap();

    assert_eq!((error, fake.requests()), (GitHubError::Permission, 1));
}

#[tokio::test(start_paused = true)]
async fn shares_public_rate_limit_cooldowns_across_requests() {
    let fake = Fake::new([Step::Respond(Ok(response(
        429,
        &[("retry-after", "30")],
        "",
    )))]);
    let github = github(&fake);

    let first = github.get(&get(3)).await.err().unwrap();
    let second = github.get(&get(3)).await.err().unwrap();

    assert!(matches!(first, GitHubError::RateLimited(_)));
    assert_eq!(second, first);
    assert_eq!(fake.requests(), 1);
}

#[tokio::test(start_paused = true)]
async fn resets_cooldown_escalation_after_a_response_that_is_not_rate_limited() {
    let fake = Fake::new([secondary_limit(&[]), ok(), secondary_limit(&[])]);
    let github = github(&fake);

    assert!(matches!(fails(&github).await, GitHubError::RateLimited(_)));
    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(github.get(&get(1)).await.unwrap().status, StatusCode::OK);

    let started = github.now();
    assert_eq!(cooldown_ms(fails(&github).await, started), 60_000);
    assert_eq!(fake.requests(), 3);
}

#[tokio::test(start_paused = true)]
async fn does_not_reset_escalation_when_a_request_admitted_before_the_limit_succeeds_later() {
    let (stale, respond) = gate();
    let fake = Fake::new([stale, secondary_limit(&[]), secondary_limit(&[])]);
    let github = github(&fake);

    let first = tokio::spawn({
        let github = github.clone();
        async move { github.get(&get(1)).await.map(|response| response.status) }
    });
    fake.reached(1).await;
    let started = github.now();
    assert_eq!(cooldown_ms(fails(&github).await, started), 60_000);

    tokio::time::advance(Duration::from_secs(60)).await;
    let _ = respond.send(Ok(response(200, &[], "")));
    assert_eq!(first.await.unwrap(), Ok(StatusCode::OK));

    let started = github.now();
    assert_eq!(cooldown_ms(fails(&github).await, started), 120_000);
    assert_eq!(fake.requests(), 3);
}

#[tokio::test(start_paused = true)]
async fn counts_concurrent_secondary_limits_once() {
    struct Case {
        first: &'static [(&'static str, &'static str)],
        second: &'static [(&'static str, &'static str)],
        gap_ms: u64,
        first_cooldown: i64,
        second_cooldown: i64,
    }
    let cases = [
        Case {
            first: &[],
            second: &[],
            gap_ms: 0,
            first_cooldown: 120_000,
            second_cooldown: 120_000,
        },
        Case {
            first: &[],
            second: &[("retry-after", "1")],
            gap_ms: 0,
            first_cooldown: 120_000,
            second_cooldown: 120_000,
        },
        Case {
            first: &[],
            second: &[("retry-after", "600")],
            gap_ms: 0,
            first_cooldown: 120_000,
            second_cooldown: 600_000,
        },
        Case {
            first: &[("retry-after", "1")],
            second: &[],
            gap_ms: 2_000,
            first_cooldown: 1_000,
            second_cooldown: 122_000,
        },
    ];
    for case in cases {
        let (first_gate, first_respond) = gate();
        let (second_gate, second_respond) = gate();
        let fake = Fake::new([
            secondary_limit(&[]),
            first_gate,
            second_gate,
            secondary_limit(&[]),
        ]);
        let github = github(&fake);
        let spawn = |github: Arc<GitHub>| {
            tokio::spawn(async move { github.get(&get(1)).await.err().unwrap() })
        };

        fails(&github).await;
        tokio::time::advance(Duration::from_secs(60)).await;
        let admitted = github.now();
        let first = spawn(github.clone());
        fake.reached(2).await;
        let second = spawn(github.clone());
        fake.reached(3).await;

        let limit = |headers| match secondary_limit(headers) {
            Step::Respond(outcome) => outcome,
            _ => unreachable!(),
        };
        let _ = first_respond.send(limit(case.first));
        let first = first.await.unwrap();
        tokio::time::advance(Duration::from_millis(case.gap_ms)).await;
        let _ = second_respond.send(limit(case.second));
        let second = second.await.unwrap();
        assert_eq!(cooldown_ms(first, admitted), case.first_cooldown);
        assert_eq!(cooldown_ms(second, admitted), case.second_cooldown);
        assert_eq!(fake.requests(), 3);

        tokio::time::advance(Duration::from_millis(
            case.second_cooldown as u64 - case.gap_ms,
        ))
        .await;
        let started = github.now();
        assert_eq!(cooldown_ms(fails(&github).await, started), 240_000);
        assert_eq!(fake.requests(), 4);
    }
}

#[tokio::test(start_paused = true)]
async fn keeps_strikes_when_a_concurrent_success_lands_with_a_zero_retry_after_limit() {
    let (stale, respond) = gate();
    let fake = Fake::new([
        stale,
        secondary_limit(&[("retry-after", "0")]),
        secondary_limit(&[]),
    ]);
    let github = github(&fake);

    let first = tokio::spawn({
        let github = github.clone();
        async move { github.get(&get(1)).await.is_ok() }
    });
    fake.reached(1).await;
    fails(&github).await;
    let _ = respond.send(Ok(response(200, &[], "")));
    assert!(first.await.unwrap());

    let started = github.now();
    assert_eq!(cooldown_ms(fails(&github).await, started), 120_000);
    assert_eq!(fake.requests(), 3);
}

#[tokio::test(start_paused = true)]
async fn keeps_cooldown_escalation_after_a_transport_failure() {
    let fake = Fake::new([
        secondary_limit(&[]),
        Step::Respond(Err(HttpError::Failed)),
        secondary_limit(&[]),
    ]);
    let github = github(&fake);

    assert!(matches!(fails(&github).await, GitHubError::RateLimited(_)));
    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(fails(&github).await, GitHubError::Http(HttpError::Failed));

    let started = github.now();
    assert_eq!(cooldown_ms(fails(&github).await, started), 120_000);
    assert_eq!(fake.requests(), 3);
}

#[tokio::test(start_paused = true)]
async fn escalates_cooldowns_across_consecutive_secondary_rate_limits() {
    let fake = Fake::new((0..6).map(|_| secondary_limit(&[])));
    let github = github(&fake);

    let mut cooldowns = Vec::new();
    for _ in 0..6 {
        let started = github.now();
        let cooldown = cooldown_ms(fails(&github).await, started);
        cooldowns.push(cooldown / 1000);
        tokio::time::advance(Duration::from_millis(cooldown as u64)).await;
    }

    assert_eq!(cooldowns, [60, 120, 240, 480, 900, 900]);
    assert_eq!(fake.requests(), 6);
}

#[tokio::test(start_paused = true)]
async fn sends_the_api_headers_and_a_valid_etag() {
    let fake = Fake::new([ok(), ok()]);
    let github = github(&fake);
    let mut request = get(1);
    request.etag = Some("W/\"etag-1\"".into());
    github.get(&request).await.unwrap();
    request.etag = Some("bad\netag".into());
    github.get(&request).await.unwrap();

    let headers = fake.headers();
    let value = |index: usize, name: &str| {
        headers[index]
            .get(name)
            .map(|value| value.to_str().unwrap().to_owned())
    };
    assert_eq!(
        value(0, "accept").as_deref(),
        Some("application/vnd.github+json")
    );
    assert_eq!(
        value(0, "x-github-api-version").as_deref(),
        Some(API_VERSION)
    );
    assert_eq!(value(0, "user-agent").as_deref(), Some("Lucent/test"));
    assert_eq!(value(0, "if-none-match").as_deref(), Some("W/\"etag-1\""));
    assert_eq!(value(1, "if-none-match"), None);
}

#[tokio::test]
async fn the_live_client_refuses_plain_http_before_connecting() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();

    let outcome = Live::default()
        .get(url, HeaderMap::new(), 1024, Duration::from_secs(10))
        .await;

    assert_eq!(outcome.err(), Some(HttpError::Failed));
    assert_eq!(
        listener.accept().err().map(|error| error.kind()),
        Some(std::io::ErrorKind::WouldBlock)
    );
}
