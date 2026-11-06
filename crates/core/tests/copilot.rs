//! Copilot provider: response mapping from fixtures, the `gh` token source with fake scripts and
//! the HTTP client against a local server.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use jiff::Timestamp;
use tamias_core::provider::copilot::{
    CopilotProvider, CopilotUser, Error, GhToken, GhTokenSource, NoQuota, Poll, TokenSource,
    parse_user,
};
use tamias_core::snapshot::{Provider, Snapshot, Source, Unit, Window};

const TOKEN: &str = "gho_TEST-TOKEN-must-never-leak";

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/copilot")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

fn now() -> Timestamp {
    ts("2026-11-04T06:00:00Z")
}

fn paid_snapshot() -> Snapshot {
    Snapshot {
        provider: Provider::Copilot,
        window: Window::Monthly,
        used: 180.0,
        limit: 300.0,
        unit: Unit::Requests,
        resets_at: ts("2026-12-01T00:00:00Z"),
        observed_at: now(),
        source: Source::Poll,
    }
}

fn body(premium: &str, reset: &str) -> Vec<u8> {
    format!(
        r#"{{"quota_snapshots":{{"premium_interactions":{premium}}},"quota_reset_date_utc":{reset}}}"#
    )
    .into_bytes()
}

// --- parse_user --------------------------------------------------------------------------------

#[test]
fn paid_fixture_yields_the_monthly_snapshot() {
    let poll = parse_user(&fixture("user-paid.json"), now()).unwrap();
    assert_eq!(poll, Poll::Quota(paid_snapshot()));
}

#[test]
fn free_seat_fixture_has_no_premium_quota() {
    let poll = parse_user(&fixture("user-ok.json"), now()).unwrap();
    assert_eq!(poll, Poll::NoQuota(NoQuota::NoPremiumQuota));
    assert_eq!(NoQuota::NoPremiumQuota.reason(), "no premium quota");
}

#[test]
fn unlimited_seat_is_no_quota() {
    let b = body(
        r#"{"unlimited":true,"has_quota":true,"entitlement":0,"remaining":0}"#,
        "null",
    );
    let poll = parse_user(&b, now()).unwrap();
    assert_eq!(poll, Poll::NoQuota(NoQuota::Unlimited));
    assert_eq!(NoQuota::Unlimited.reason(), "unlimited");
}

#[test]
fn zero_entitlement_with_quota_flag_is_no_premium_quota() {
    let b = body(
        r#"{"unlimited":false,"has_quota":true,"entitlement":0,"remaining":0}"#,
        r#""2026-12-01T00:00:00Z""#,
    );
    assert_eq!(
        parse_user(&b, now()).unwrap(),
        Poll::NoQuota(NoQuota::NoPremiumQuota)
    );
}

#[test]
#[allow(clippy::float_cmp)]
fn remaining_above_entitlement_clamps_used_at_zero() {
    let b = body(
        r#"{"unlimited":false,"has_quota":true,"entitlement":300,"remaining":350}"#,
        r#""2026-12-01T00:00:00Z""#,
    );
    let Poll::Quota(s) = parse_user(&b, now()).unwrap() else {
        panic!("expected a quota");
    };
    assert_eq!(s.used, 0.0);
    assert_eq!(s.limit, 300.0);
}

#[test]
fn changed_shapes_are_schema_errors() {
    let ok = r#"{"unlimited":false,"has_quota":true,"entitlement":300,"remaining":120}"#;
    let date = r#""2026-12-01T00:00:00Z""#;
    let bodies: Vec<Vec<u8>> = vec![
        br#"{"quota_snapshots":{"chat":{}},"quota_reset_date_utc":"2026-12-01T00:00:00Z"}"#
            .to_vec(),
        br#"{"quota_snapshots":null}"#.to_vec(),
        b"{}".to_vec(),
        b"not json".to_vec(),
        body(
            r#"{"unlimited":false,"has_quota":true,"entitlement":"300","remaining":120}"#,
            date,
        ),
        body(
            r#"{"unlimited":false,"has_quota":true,"entitlement":300}"#,
            date,
        ),
        body(
            r#"{"unlimited":false,"has_quota":true,"entitlement":-1,"remaining":1}"#,
            date,
        ),
        body(
            r#"{"has_quota":true,"entitlement":300,"remaining":1}"#,
            date,
        ),
        body(ok, r#""next month""#),
        body(ok, "null"),
        body(ok, "12"),
    ];
    for b in &bodies {
        let result = parse_user(b, now());
        assert!(
            matches!(result, Err(Error::Schema(_))),
            "{:?} -> {result:?}",
            String::from_utf8_lossy(b)
        );
    }
}

#[test]
fn unauthorized_fixture_as_body_is_a_schema_error() {
    let result = parse_user(&fixture("user-unauthorized.json"), now());
    assert!(matches!(result, Err(Error::Schema(_))), "{result:?}");
}

#[test]
fn error_classes_are_stable() {
    assert_eq!(Error::NoCredentials("x".into()).class(), "no credentials");
    assert_eq!(Error::Unauthorized.class(), "unauthorized");
    assert_eq!(Error::Timeout("x".into()).class(), "timeout");
    assert_eq!(Error::Schema("x".into()).class(), "schema");
    assert_eq!(
        Error::Http("unexpected HTTP status 500".into()).class(),
        "http 500"
    );
    assert_eq!(Error::Http("refused".into()).class(), "network");
}

#[test]
fn token_never_appears_in_debug_output() {
    let token = GhToken::new(TOKEN.to_owned());
    let shown = format!("{token:?} {token:#?}");
    assert!(!shown.contains("TEST-TOKEN"), "{shown}");
    assert!(shown.contains("redacted"));
}

// --- GhTokenSource: resolution ---------------------------------------------------------------

fn touch(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "").unwrap();
    path
}

fn gh_name() -> &'static str {
    if cfg!(windows) { "gh.exe" } else { "gh" }
}

fn source(explicit: Option<PathBuf>, path_dirs: &[&Path], known: Vec<PathBuf>) -> GhTokenSource {
    let path_var: OsString = std::env::join_paths(path_dirs).unwrap();
    GhTokenSource::with_search(explicit, Some(path_var), known, Duration::from_secs(5))
}

#[test]
fn explicit_path_wins_over_path_and_known_locations() {
    let dir = tempfile::tempdir().unwrap();
    let explicit = touch(dir.path(), "my-gh");
    let on_path = tempfile::tempdir().unwrap();
    touch(on_path.path(), gh_name());
    let known = touch(dir.path(), "known-gh");
    let s = source(Some(explicit.clone()), &[on_path.path()], vec![known]);
    assert_eq!(s.resolve(), Some(explicit));
}

#[test]
fn missing_explicit_path_does_not_fall_back() {
    let dir = tempfile::tempdir().unwrap();
    let on_path = tempfile::tempdir().unwrap();
    touch(on_path.path(), gh_name());
    let s = source(Some(dir.path().join("absent")), &[on_path.path()], vec![]);
    assert_eq!(s.resolve(), None);
    assert!(matches!(s.gh_token(), Err(Error::NoCredentials(_))));
}

#[test]
fn path_wins_over_known_locations() {
    let on_path = tempfile::tempdir().unwrap();
    let expected = touch(on_path.path(), gh_name());
    let dir = tempfile::tempdir().unwrap();
    let known = touch(dir.path(), "known-gh");
    let s = source(None, &[dir.path(), on_path.path()], vec![known]);
    assert_eq!(s.resolve(), Some(expected));
}

#[test]
fn known_locations_are_the_last_resort_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let second = touch(dir.path(), "second");
    let third = touch(dir.path(), "third");
    let s = source(
        None,
        &[empty.path()],
        vec![dir.path().join("first"), second.clone(), third],
    );
    assert_eq!(s.resolve(), Some(second));
}

#[test]
fn nothing_found_is_no_credentials() {
    let empty = tempfile::tempdir().unwrap();
    let s = source(None, &[empty.path()], vec![empty.path().join("gh")]);
    assert_eq!(s.resolve(), None);
    let err = s.gh_token().unwrap_err();
    assert!(matches!(err, Error::NoCredentials(_)), "{err:?}");
}

// --- local HTTP server -------------------------------------------------------------------------

fn serve_once(response: Vec<u8>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/copilot_internal/user",
        listener.local_addr().unwrap()
    );
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

fn client(url: &str) -> CopilotUser {
    CopilotUser::with_url(url, Duration::from_secs(5))
}

fn token() -> GhToken {
    GhToken::new(TOKEN.to_owned())
}

// --- GhTokenSource: fake gh (unix scripts) -----------------------------------------------------

#[cfg(unix)]
mod fake_gh {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Mutex, MutexGuard};

    /// Serializes the tests that write and spawn scripts: a fork in another test while a script
    /// is still open for writing makes the exec fail with "text file busy".
    static SPAWN: Mutex<()> = Mutex::new(());

    fn lock() -> MutexGuard<'static, ()> {
        SPAWN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("gh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn run(path: PathBuf, timeout: Duration) -> Result<GhToken, Error> {
        GhTokenSource::with_search(Some(path), None, vec![], timeout).gh_token()
    }

    #[test]
    fn token_is_read_and_trimmed_with_the_right_arguments() {
        let _guard = lock();
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            &format!(
                r#"[ "$1 $2" = "auth token" ] || exit 3
echo "  {TOKEN}  "
echo "noise" >&2"#
            ),
        );
        let gh = GhTokenSource::with_search(Some(path), None, vec![], Duration::from_secs(5));
        let (url, head) = serve_once(http_response("200 OK", &fixture("user-paid.json")));
        let provider = CopilotProvider::new(gh, client(&url));
        assert_eq!(provider.poll(now()).unwrap(), Poll::Quota(paid_snapshot()));
        let head = head.recv_timeout(Duration::from_secs(5)).unwrap();
        // Trimmed: the header ends right after the token.
        assert!(
            head.contains(&format!("authorization: Bearer {TOKEN}\r\n"))
                || head.contains(&format!("Authorization: Bearer {TOKEN}\r\n")),
            "{head}"
        );
    }

    #[test]
    fn token_is_found_on_path() {
        let _guard = lock();
        let dir = tempfile::tempdir().unwrap();
        script(dir.path(), &format!("echo {TOKEN}"));
        let s = GhTokenSource::with_search(
            None,
            Some(dir.path().as_os_str().to_owned()),
            vec![],
            Duration::from_secs(5),
        );
        assert!(s.gh_token().is_ok());
    }

    #[test]
    fn non_zero_exit_is_no_credentials_without_leaking() {
        let _guard = lock();
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            &format!("echo {TOKEN}\necho {TOKEN} >&2\nexit 1"),
        );
        let err = run(path, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, Error::NoCredentials(_)), "{err:?}");
        assert!(!format!("{err} {err:?}").contains("TEST-TOKEN"));
    }

    #[test]
    fn empty_output_is_no_credentials() {
        let _guard = lock();
        let dir = tempfile::tempdir().unwrap();
        let path = script(dir.path(), "echo '   '");
        let err = run(path, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, Error::NoCredentials(_)), "{err:?}");
    }

    #[test]
    fn hanging_gh_times_out_and_is_killed() {
        let _guard = lock();
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let path = script(
            dir.path(),
            &format!("echo $$ > '{}'\nexec sleep 30", pid_file.display()),
        );
        let started = std::time::Instant::now();
        let err = run(path, Duration::from_millis(500)).unwrap_err();
        assert!(matches!(err, Error::Timeout(_)), "{err:?}");
        assert_eq!(err.class(), "timeout");
        assert!(started.elapsed() < Duration::from_secs(5));

        let pid: u32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = tamias_core::process::command("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive, "gh child {pid} is still running");
    }
}

// --- CopilotUser against a local server --------------------------------------------------------

#[test]
fn fetch_sends_the_headers_and_maps_the_response() {
    let (url, head) = serve_once(http_response("200 OK", &fixture("user-paid.json")));
    let poll = client(&url).fetch(token(), now()).unwrap();
    assert_eq!(poll, Poll::Quota(paid_snapshot()));

    let head = head
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .to_lowercase();
    assert!(head.starts_with("get /copilot_internal/user "), "{head}");
    assert!(head.contains(&format!("authorization: bearer {}", TOKEN.to_lowercase())));
    assert!(head.contains("accept: application/vnd.github+json"));
    assert!(head.contains(&format!("user-agent: tamias/{}", tamias_core::version())));
}

#[test]
fn http_401_is_unauthorized() {
    let (url, _) = serve_once(http_response(
        "401 Unauthorized",
        &fixture("user-unauthorized.json"),
    ));
    let err = client(&url).fetch(token(), now()).unwrap_err();
    assert!(matches!(err, Error::Unauthorized), "{err:?}");
    assert_eq!(err.class(), "unauthorized");
}

#[test]
fn http_500_is_http_error_with_class() {
    let (url, _) = serve_once(http_response("500 Internal Server Error", b"{}"));
    let err = client(&url).fetch(token(), now()).unwrap_err();
    assert!(matches!(err, Error::Http(_)), "{err:?}");
    assert_eq!(err.class(), "http 500");
    assert!(!format!("{err} {err:?}").contains("TEST-TOKEN"));
}

#[test]
fn changed_shape_over_http_is_schema_error() {
    let (url, _) = serve_once(http_response("200 OK", b"{}"));
    let err = client(&url).fetch(token(), now()).unwrap_err();
    assert!(matches!(err, Error::Schema(_)), "{err:?}");
}

#[test]
fn connection_refused_is_network_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/x", listener.local_addr().unwrap());
    drop(listener);
    let err = client(&url).fetch(token(), now()).unwrap_err();
    assert_eq!(err.class(), "network");
}

#[test]
fn silent_server_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/x", listener.local_addr().unwrap());
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_secs(3));
        drop(stream);
    });
    let result = CopilotUser::with_url(url, Duration::from_millis(300)).fetch(token(), now());
    assert!(matches!(result, Err(Error::Timeout(_))), "{result:?}");
}

// --- CopilotProvider ---------------------------------------------------------------------------

#[derive(Debug)]
struct FixedSource(Option<&'static str>);

impl TokenSource for FixedSource {
    fn gh_token(&self) -> Result<GhToken, Error> {
        self.0
            .map(|t| GhToken::new(t.to_owned()))
            .ok_or_else(|| Error::NoCredentials("not logged in".to_owned()))
    }
}

#[test]
fn provider_combines_source_and_client() {
    let (url, _) = serve_once(http_response("200 OK", &fixture("user-paid.json")));
    let provider = CopilotProvider::new(FixedSource(Some(TOKEN)), client(&url));
    assert_eq!(provider.poll(now()).unwrap(), Poll::Quota(paid_snapshot()));
}

#[test]
fn provider_without_token_fails_without_a_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/x", listener.local_addr().unwrap());
    let provider = CopilotProvider::new(FixedSource(None), client(&url));
    assert!(matches!(provider.poll(now()), Err(Error::NoCredentials(_))));
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err(), "a request was made");
}
