
use reply_voice::config::{Config, ConfigError};
use reply_voice::receiver;
use reply_voice::speaker::{Engine, Speaker, espeak_wpm, windows_rate};
use reply_voice::status::{Bridge, Shared};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Recorder(Arc<Mutex<Vec<(String, f32)>>>);
impl Engine for Recorder {
    fn speak(&mut self, text: &str, speed: f32) -> Result<(), String> {
        self.0.lock().unwrap().push((text.to_string(), speed));
        Ok(())
    }
    fn stop(&mut self) {}
}

const TOKEN: &str = "0123456789abcdef0123";

fn setup() -> (receiver::Handle, Arc<Mutex<Vec<(String, f32)>>>, Shared) {
    let log = Arc::new(Mutex::new(vec![]));
    let shared = Shared::default();
    let sp = Arc::new(Speaker::start(
        Box::new(Recorder(log.clone())),
        shared.clone(),
        1.25,
    ));
    let h = receiver::start(
        "127.0.0.1:0".parse().unwrap(),
        TOKEN.into(),
        sp,
        shared.clone(),
    )
    .unwrap();
    (h, log, shared)
}

fn request(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    auth: Option<&str>,
    body: &str,
) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(a) = auth {
        req.push_str(&format!("Authorization: Bearer {a}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split(' ').nth(1).unwrap().parse().unwrap();
    (code, out)
}

fn wait_for(log: &Arc<Mutex<Vec<(String, f32)>>>, n: usize) {
    let t = Instant::now();
    while log.lock().unwrap().len() < n && t.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn speaks_only_the_reply_text_with_the_saved_speed() {
    let (h, log, shared) = setup();
    assert_eq!(shared.get().bridge, Bridge::Listening);
    let (c, _) = request(
        h.addr,
        "POST",
        "/speak",
        Some(TOKEN),
        r#"{"text":"Hello there","extra":"ignored"}"#,
    );
    assert_eq!(c, 202);
    wait_for(&log, 1);
    assert_eq!(log.lock().unwrap()[0], ("Hello there".to_string(), 1.25));
    assert_eq!(shared.get().bridge, Bridge::Connected);
    assert!(shared.get().last_reply_at.is_some());
    h.stop();
}

#[test]
fn wrong_or_missing_token_is_refused_and_not_marked_connected() {
    let (h, log, shared) = setup();
    assert_eq!(
        request(h.addr, "POST", "/speak", Some("nope"), r#"{"text":"x"}"#).0,
        401
    );
    assert_eq!(
        request(h.addr, "POST", "/speak", None, r#"{"text":"x"}"#).0,
        401
    );
    assert_eq!(
        request(
            h.addr,
            "POST",
            "/speak",
            Some(&format!("{TOKEN}0")),
            r#"{"text":"x"}"#
        )
        .0,
        401
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(log.lock().unwrap().is_empty());
    assert_eq!(shared.get().bridge, Bridge::Listening);
    h.stop();
}

#[test]
fn bad_payloads_are_rejected() {
    let (h, log, _s) = setup();
    assert_eq!(
        request(h.addr, "POST", "/speak", Some(TOKEN), "not json").0,
        400
    );
    assert_eq!(
        request(h.addr, "POST", "/speak", Some(TOKEN), r#"{"text":"   "}"#).0,
        400
    );
    let long = format!(
        r#"{{"text":"{}"}}"#,
        "a".repeat(receiver::MAX_TEXT_BYTES + 1)
    );
    assert_eq!(request(h.addr, "POST", "/speak", Some(TOKEN), &long).0, 400);
    let huge = format!(
        r#"{{"text":"{}"}}"#,
        "a".repeat(receiver::MAX_BODY_BYTES + 10)
    );
    assert_eq!(request(h.addr, "POST", "/speak", Some(TOKEN), &huge).0, 413);
    assert_eq!(request(h.addr, "GET", "/speak", Some(TOKEN), "").0, 405);
    assert_eq!(request(h.addr, "POST", "/other", Some(TOKEN), "{}").0, 404);
    std::thread::sleep(Duration::from_millis(100));
    assert!(log.lock().unwrap().is_empty());
    h.stop();
}

#[test]
fn health_needs_no_token_and_leaks_nothing() {
    let (h, _l, shared) = setup();
    let (c, body) = request(h.addr, "GET", "/health", None, "");
    assert_eq!(c, 200);
    assert!(body.ends_with(r#"{"ok":true}"#));
    assert_eq!(shared.get().bridge, Bridge::Listening);
    h.stop();
}

#[test]
fn replies_are_read_in_order() {
    let (h, log, _s) = setup();
    for i in 0..5 {
        assert_eq!(
            request(
                h.addr,
                "POST",
                "/speak",
                Some(TOKEN),
                &format!(r#"{{"text":"m{i}"}}"#)
            )
            .0,
            202
        );
    }
    wait_for(&log, 5);
    let got: Vec<String> = log.lock().unwrap().iter().map(|x| x.0.clone()).collect();
    assert_eq!(got, vec!["m0", "m1", "m2", "m3", "m4"]);
    h.stop();
}

#[test]
fn receiver_refuses_non_loopback_and_empty_token() {
    let shared = Shared::default();
    let log = Arc::new(Mutex::new(vec![]));
    let sp = Arc::new(Speaker::start(Box::new(Recorder(log)), shared.clone(), 1.0));
    assert!(
        receiver::start(
            "0.0.0.0:0".parse().unwrap(),
            TOKEN.into(),
            sp.clone(),
            shared.clone()
        )
        .is_err()
    );
    assert!(receiver::start("127.0.0.1:0".parse().unwrap(), String::new(), sp, shared).is_err());
}

#[test]
fn config_validation_and_clamp() {
    let mut c = Config::default();
    assert!(c.validate().is_ok());
    assert!(!c.link_configured());
    c.token = "short".into();
    assert_eq!(c.validate(), Err(ConfigError::TokenTooShort));
    c.token = TOKEN.into();
    assert!(c.link_configured());
    c.address = "0.0.0.0:47615".into();
    assert_eq!(c.validate(), Err(ConfigError::NotLoopback));
    c.address = "example.com".into();
    assert_eq!(c.validate(), Err(ConfigError::BadAddress));
    c.speed = 9.0;
    c.clamp();
    assert_eq!(c.speed, 1.5);
    c.speed = f32::NAN;
    c.clamp();
    assert_eq!(c.speed, 1.0);
}

#[test]
fn config_round_trip_and_bad_file_falls_back() {
    let dir = std::env::temp_dir().join(format!("rv-test-{}", std::process::id()));
    let p = dir.join("config.json");
    let mut c = Config::default();
    c.token = TOKEN.into();
    c.speed = 1.25;
    c.save(&p).unwrap();
    assert_eq!(Config::load(&p), c);
    std::fs::write(&p, "{broken").unwrap();
    assert_eq!(Config::load(&p), Config::default());
    #[cfg(unix)]
    {
        c.save(&p).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn speed_maps() {
    assert_eq!(windows_rate(1.0), 0);
    assert_eq!(windows_rate(1.5), 10);
    assert_eq!(windows_rate(0.75), -5);
    assert_eq!(espeak_wpm(1.0), 175);
    assert_eq!(espeak_wpm(1.5), 263);
}

#[test]
fn generated_tokens_are_long_and_differ() {
    let a = reply_voice::config::generate_token();
    let b = reply_voice::config::generate_token();
    assert!(a.len() >= 16 && a != b);
}

#[cfg(not(windows))]
#[test]
fn autostart_writes_and_removes_entry() {
    let dir = std::env::temp_dir().join(format!("rv-auto-{}", std::process::id()));
    // Only this test touches this variable.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
    reply_voice::autostart::set(true).unwrap();
    let p = reply_voice::autostart::desktop_path().unwrap();
    let body = std::fs::read_to_string(&p).unwrap();
    assert!(body.contains("Exec=\"") && body.contains("Reply Voice"));
    reply_voice::autostart::set(false).unwrap();
    assert!(!p.exists());
    reply_voice::autostart::set(false).unwrap(); // removing twice is fine
    let _ = std::fs::remove_dir_all(dir);
}
