//! Claude provider: response mapping from fixtures, the HTTP client against a local server and
//! credential parsing / the file source.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use jiff::Timestamp;
use tamias_core::credentials::{CredentialSource, FileSource, Secret, parse_credentials};
use tamias_core::provider::claude::{ClaudeProvider, Error, UsageClient, parse_usage};
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};

const TOKEN: &str = "sk-ant-oat01-TEST-TOKEN-must-never-leak";

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn now() -> Timestamp {
    ts("2026-11-02T06:00:00Z")
}

fn valid_token() -> Secret {
    Secret::new(TOKEN.to_owned(), ts("2026-11-02T12:00:00Z"))
}

fn expected(window: Window, used: f64, resets_at: &str) -> Snapshot {
    Snapshot {
        provider: Provider::Claude,
        window,
        used,
        limit: 100.0,
        unit: Unit::Percent,
        resets_at: ts(resets_at),
        observed_at: now(),
        source: Source::Poll,
    }
}

fn assert_no_token(err: &Error) {
    let shown = format!("{err} / {err:?}");
    assert!(!shown.contains(TOKEN), "token leaked: {shown}");
    assert!(!shown.contains("TEST-TOKEN"), "token leaked: {shown}");
}

// --- parse_usage -----------------------------------------------------------------------------

#[test]
fn usage_ok_fixture_yields_both_windows_unrounded() {
    let snapshots = parse_usage(&fixture("usage-ok.json"), now()).unwrap();
    assert_eq!(
        snapshots,
        vec![
            expected(Window::FiveHour, 42.0, "2026-11-02T07:40:00.155067Z"),
            expected(Window::SevenDay, 17.0, "2026-11-02T21:00:00.155085Z"),
        ]
    );
}

#[test]
fn after_reset_fixture_without_breakdown_and_fraction_is_accepted() {
    let snapshots = parse_usage(&fixture("usage-after-reset.json"), now()).unwrap();
    assert_eq!(
        snapshots[1],
        expected(Window::SevenDay, 0.0, "2026-11-09T21:00:00Z")
    );
    assert_eq!(snapshots[0].window, Window::FiveHour);
}

#[test]
fn offset_in_resets_at_is_honoured() {
    let body = br#"{"five_hour":{"utilization":1.5,"resets_at":"2026-11-02T08:40:00.155067+01:00"},
                    "seven_day":{"utilization":2,"resets_at":"2026-11-02T21:00:00Z"},"new_key":{"x":1}}"#;
    let snapshots = parse_usage(body, now()).unwrap();
    assert_eq!(snapshots[0].resets_at, ts("2026-11-02T07:40:00.155067Z"));
    assert!((snapshots[0].used - 1.5).abs() < f64::EPSILON);
    assert!((snapshots[1].used - 2.0).abs() < f64::EPSILON);
}

#[test]
fn unknown_or_changed_responses_are_schema_errors() {
    let cases: &[&[u8]] = &[
        b"",
        b"not json",
        b"[]",
        b"null",
        b"{}",
        br#"{"five_hour":null,"seven_day":null}"#,
        // one valid window and one broken: no partial result
        br#"{"five_hour":{"utilization":1.0,"resets_at":"2026-11-02T07:40:00Z"},"seven_day":null}"#,
        br#"{"five_hour":{"utilization":"42","resets_at":"2026-11-02T07:40:00Z"},"seven_day":{"utilization":1.0,"resets_at":"2026-11-02T21:00:00Z"}}"#,
        br#"{"five_hour":{"utilization":null,"resets_at":"2026-11-02T07:40:00Z"},"seven_day":{"utilization":1.0,"resets_at":"2026-11-02T21:00:00Z"}}"#,
        br#"{"five_hour":{"utilization":-1.0,"resets_at":"2026-11-02T07:40:00Z"},"seven_day":{"utilization":1.0,"resets_at":"2026-11-02T21:00:00Z"}}"#,
        br#"{"five_hour":{"utilization":1.0,"resets_at":"tomorrow"},"seven_day":{"utilization":1.0,"resets_at":"2026-11-02T21:00:00Z"}}"#,
        br#"{"five_hour":{"utilization":1.0},"seven_day":{"utilization":1.0,"resets_at":"2026-11-02T21:00:00Z"}}"#,
        br#"{"five_hour":{"utilization":1.0,"resets_at":1762000000},"seven_day":{"utilization":1.0,"resets_at":"2026-11-02T21:00:00Z"}}"#,
    ];
    for body in cases {
        let result = parse_usage(body, now());
        assert!(
            matches!(result, Err(Error::Schema(_))),
            "{:?} -> {result:?}",
            String::from_utf8_lossy(body)
        );
    }
}

#[test]
fn unauthorized_fixture_as_body_is_a_schema_error() {
    let result = parse_usage(&fixture("usage-unauthorized.json"), now());
    assert!(matches!(result, Err(Error::Schema(_))), "{result:?}");
}

// --- UsageClient against a local server ------------------------------------------------------

/// Serves one connection with `response` (raw HTTP) and sends the received request head back.
fn serve_once(response: Vec<u8>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let head = read_head(&mut stream);
        stream.write_all(&response).unwrap();
        let _ = tx.send(head);
    });
    (url, rx)
}

fn read_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap() == 0 {
            break;
        }
        head.push(byte[0]);
    }
    String::from_utf8(head).unwrap()
}

fn http_response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

fn client(url: &str) -> UsageClient {
    UsageClient::with_url(url, Duration::from_secs(5))
}

#[test]
fn fetch_sends_bearer_and_beta_header_and_maps_the_response() {
    let (url, head) = serve_once(http_response("200 OK", &fixture("usage-ok.json")));
    let snapshots = client(&url).fetch(valid_token(), now()).unwrap();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(
        snapshots[1],
        expected(Window::SevenDay, 17.0, "2026-11-02T21:00:00.155085Z")
    );

    let head = head
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .to_lowercase();
    assert!(head.starts_with("get /api/oauth/usage "), "{head}");
    assert!(head.contains(&format!(
        "authorization: bearer {}\r\n",
        TOKEN.to_lowercase()
    )));
    assert!(head.contains("anthropic-beta: oauth-2025-04-20\r\n"));
}

#[test]
fn status_401_is_unauthorized_and_tells_to_run_claude_code() {
    let (url, _) = serve_once(http_response(
        "401 Unauthorized",
        &fixture("usage-unauthorized.json"),
    ));
    let err = client(&url).fetch(valid_token(), now()).unwrap_err();
    assert!(matches!(err, Error::Unauthorized), "{err:?}");
    assert_eq!(err.class(), "unauthorized");
    assert!(err.to_string().contains("run Claude Code once"));
    assert_no_token(&err);
}

#[test]
fn other_status_is_http_error() {
    let (url, _) = serve_once(http_response("500 Internal Server Error", b"{}"));
    let err = client(&url).fetch(valid_token(), now()).unwrap_err();
    assert!(
        matches!(err, Error::Http(ref m) if m.contains("500")),
        "{err:?}"
    );
    assert_eq!(err.class(), "http 500");
    assert_no_token(&err);
}

#[test]
fn changed_body_with_200_is_schema_error() {
    let (url, _) = serve_once(http_response("200 OK", br#"{"five_hour":{"pct":3}}"#));
    let err = client(&url).fetch(valid_token(), now()).unwrap_err();
    assert!(matches!(err, Error::Schema(_)), "{err:?}");
}

#[test]
fn connection_refused_is_http_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", listener.local_addr().unwrap());
    drop(listener);
    let err = client(&url).fetch(valid_token(), now()).unwrap_err();
    assert!(matches!(err, Error::Http(_)), "{err:?}");
    assert_eq!(err.class(), "network");
    assert_no_token(&err);
}

#[test]
fn silent_server_times_out_with_typed_error_and_no_snapshot() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", listener.local_addr().unwrap());
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_secs(5));
        drop(stream);
    });
    let started = std::time::Instant::now();
    let result = UsageClient::with_url(url, Duration::from_millis(300)).fetch(valid_token(), now());
    assert!(matches!(result, Err(Error::Timeout(_))), "{result:?}");
    assert_eq!(result.unwrap_err().class(), "timeout");
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn expired_token_is_rejected_without_a_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/oauth/usage", listener.local_addr().unwrap());
    let expired = Secret::new(TOKEN.to_owned(), now());
    let err = client(&url).fetch(expired, now()).unwrap_err();
    assert!(
        matches!(err, Error::Expired { expires_at } if expires_at == now()),
        "{err:?}"
    );
    assert!(err.to_string().contains("run Claude Code once"));
    assert_no_token(&err);

    listener.set_nonblocking(true).unwrap();
    let accepted = listener.accept();
    assert!(
        matches!(accepted, Err(ref e) if e.kind() == ErrorKind::WouldBlock),
        "a request was made: {accepted:?}"
    );
}

// --- ClaudeProvider ----------------------------------------------------------------------------

#[derive(Debug)]
struct FakeSource(fn() -> tamias_core::provider::claude::Result<Secret>);

impl CredentialSource for FakeSource {
    fn claude_token(&self) -> tamias_core::provider::claude::Result<Secret> {
        (self.0)()
    }
}

#[test]
fn poll_combines_source_and_client() {
    let (url, _) = serve_once(http_response("200 OK", &fixture("usage-ok.json")));
    let provider = ClaudeProvider::new(FakeSource(|| Ok(valid_token())), client(&url));
    let snapshots = provider.poll(now()).unwrap();
    assert_eq!(
        snapshots[0],
        expected(Window::FiveHour, 42.0, "2026-11-02T07:40:00.155067Z")
    );
}

#[test]
fn poll_propagates_a_credential_error_without_snapshots() {
    let provider = ClaudeProvider::new(
        FakeSource(|| Err(Error::NoCredentials("gone".to_owned()))),
        UsageClient::with_url("http://127.0.0.1:9/never", Duration::from_secs(1)),
    );
    let result = provider.poll(now());
    assert!(matches!(result, Err(Error::NoCredentials(_))), "{result:?}");
}

// --- credentials -------------------------------------------------------------------------------

fn credentials_json(token: &str, expires_at_ms: i64) -> String {
    format!(
        r#"{{"claudeAiOauth":{{"accessToken":"{token}","refreshToken":"r","expiresAt":{expires_at_ms},"scopes":["user:profile"],"subscriptionType":"max"}},"other":null}}"#
    )
}

#[test]
fn credentials_yield_token_with_expiry_from_epoch_millis() {
    let secret = parse_credentials(credentials_json(TOKEN, 1_762_084_800_000).as_bytes()).unwrap();
    assert_eq!(secret.expires_at(), ts("2025-11-02T12:00:00Z"));
    assert!(secret.is_expired(ts("2025-11-02T12:00:00Z")));
    assert!(!secret.is_expired(ts("2025-11-02T11:59:59Z")));
}

#[test]
fn secret_debug_hides_the_value() {
    let shown = format!("{:?}", valid_token());
    assert!(!shown.contains(TOKEN), "{shown}");
    assert!(shown.contains("redacted"));
}

#[test]
fn api_key_login_without_oauth_entry_is_no_credentials() {
    for json in [
        r#"{"primaryApiKey":"x"}"#,
        r#"{"claudeAiOauth":null}"#,
        r#"{"claudeAiOauth":{"accessToken":"","expiresAt":1}}"#,
    ] {
        let result = parse_credentials(json.as_bytes());
        assert!(
            matches!(result, Err(Error::NoCredentials(_))),
            "{json} -> {result:?}"
        );
    }
}

#[test]
fn malformed_credentials_are_schema_errors_without_the_token() {
    for json in [
        String::new(),
        format!(r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","expiresAt":"{TOKEN}"}}}}"#),
        format!(r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}"}}}}"#),
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","expiresAt":1}}}} trailing {TOKEN}"#
        ),
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","expiresAt":9223372036854775807}}}}"#
        ),
    ] {
        let err = parse_credentials(json.as_bytes()).unwrap_err();
        assert!(matches!(err, Error::Schema(_)), "{err:?}");
        assert_no_token(&err);
    }
}

#[test]
fn file_path_prefers_claude_config_dir_over_home() {
    let home = PathBuf::from("home");
    assert_eq!(
        FileSource::resolve(Some("cfg".into()), Some(home.clone())),
        Some(PathBuf::from("cfg").join(".credentials.json"))
    );
    assert_eq!(
        FileSource::resolve(None, Some(home.clone())),
        Some(home.join(".claude").join(".credentials.json"))
    );
    assert_eq!(
        FileSource::resolve(Some("".into()), Some(home.clone())),
        Some(home.join(".claude").join(".credentials.json"))
    );
    assert_eq!(FileSource::resolve(None, None), None);
}

#[test]
fn file_source_reads_credentials_from_temp_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".credentials.json"),
        credentials_json(TOKEN, 1_762_084_800_000),
    )
    .unwrap();
    let path = FileSource::resolve(Some(dir.path().into()), None).unwrap();
    let secret = FileSource::new(path).claude_token().unwrap();
    assert_eq!(secret.expires_at(), ts("2025-11-02T12:00:00Z"));
}

#[test]
fn file_source_reads_credentials_from_temp_home() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    std::fs::write(
        home.path().join(".claude").join(".credentials.json"),
        credentials_json(TOKEN, 1_762_084_800_000),
    )
    .unwrap();
    let path = FileSource::resolve(None, Some(home.path().to_path_buf())).unwrap();
    assert!(FileSource::new(path).claude_token().is_ok());
}

#[test]
fn missing_credentials_file_is_no_credentials_naming_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".credentials.json");
    let err = FileSource::new(&path).claude_token().unwrap_err();
    assert!(
        matches!(err, Error::NoCredentials(ref m) if m.contains("not found")),
        "{err:?}"
    );
    assert!(err.to_string().contains(&path.display().to_string()));
}

#[test]
fn file_without_oauth_entry_is_no_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".credentials.json");
    std::fs::write(&path, r#"{"primaryApiKey":"x"}"#).unwrap();
    let result = FileSource::new(&path).claude_token();
    assert!(matches!(result, Err(Error::NoCredentials(_))), "{result:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_default_source_is_the_keychain_via_usr_bin_security() {
    let shown = format!("{:?}", tamias_core::credentials::os_default().unwrap());
    assert!(shown.contains("/usr/bin/security"), "{shown}");
    assert!(shown.contains("Claude Code-credentials"), "{shown}");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_default_source_is_the_credentials_file() {
    let shown = format!("{:?}", tamias_core::credentials::os_default().unwrap());
    assert!(shown.contains(".credentials.json"), "{shown}");
}

// --- Error::class ----------------------------------------------------------------------------

#[test]
fn error_class_is_short_and_never_carries_the_message() {
    let cases = [
        (
            Error::NoCredentials("/home/u/.claude/.credentials.json".to_owned()),
            "no credentials",
        ),
        (Error::Expired { expires_at: now() }, "expired"),
        (Error::Unauthorized, "unauthorized"),
        (Error::Timeout("security".to_owned()), "timeout"),
        (
            Error::Http("unexpected HTTP status 503".to_owned()),
            "http 503",
        ),
        (
            Error::Http("unexpected HTTP status abc".to_owned()),
            "network",
        ),
        (Error::Http("io: connection reset".to_owned()), "network"),
        (Error::Schema(format!("body {TOKEN}")), "schema"),
    ];
    for (err, class) in cases {
        assert_eq!(err.class(), class, "{err:?}");
    }
}
