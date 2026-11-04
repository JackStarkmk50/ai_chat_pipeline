
use reply_voice::speaker::Engine;
use reply_voice::voice::{self, Download, Installer, Item, Manifest};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rv-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn wait_done(i: &Installer) -> Download {
    let t = Instant::now();
    loop {
        let s = i.get();
        if !matches!(s, Download::Downloading { .. }) || t.elapsed() > Duration::from_secs(120) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn item(path: &Path, file: &str, extract: bool) -> Item {
    Item {
        url: format!("file://{}", path.display()),
        sha256: voice::sha256_file(path).unwrap(),
        size: std::fs::metadata(path).unwrap().len(),
        file: file.into(),
        extract,
    }
}

#[cfg(unix)]
fn fixtures(dir: &Path) -> Manifest {
    let pdir = dir.join("src/piper");
    std::fs::create_dir_all(&pdir).unwrap();
    std::fs::write(pdir.join("piper"), "#!/bin/sh\ncat >/dev/null\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(pdir.join("piper"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let tgz = dir.join("piper.tgz");
    assert!(
        Command::new("tar")
            .arg("czf")
            .arg(&tgz)
            .arg("-C")
            .arg(dir.join("src"))
            .arg("piper")
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(dir.join("m.onnx"), b"model").unwrap();
    std::fs::write(dir.join("m.json"), b"{}").unwrap();
    Manifest {
        name: "Test voice".into(),
        model_file: "m.onnx".into(),
        items: vec![
            item(&tgz, "piper.tgz", true),
            item(&dir.join("m.json"), "m.onnx.json", false),
            item(&dir.join("m.onnx"), "m.onnx", false),
        ],
    }
}

#[cfg(unix)]
#[test]
fn installer_installs_when_checksums_match() {
    let d = tmp("inst");
    let m = fixtures(&d);
    let base = d.join("out");
    assert!(!voice::installed(&base));
    let i = Installer::default();
    i.start(base.clone(), m);
    assert_eq!(wait_done(&i), Download::Done);
    assert!(voice::installed(&base));
    assert_eq!(voice::installed_manifest(&base).unwrap().name, "Test voice");
    assert!(
        !base.join("piper.tgz").exists(),
        "archive is removed after unpacking"
    );
}

#[cfg(unix)]
#[test]
fn installer_rejects_a_file_with_the_wrong_checksum() {
    let d = tmp("bad");
    let mut m = fixtures(&d);
    m.items[2].sha256 = "0".repeat(64);
    let base = d.join("out");
    let i = Installer::default();
    i.start(base.clone(), m);
    match wait_done(&i) {
        Download::Failed(m) => assert!(m.contains("checksum"), "{m}"),
        other => panic!("expected failure, got {other:?}"),
    }
    assert!(!voice::installed(&base));
    assert!(!base.join("m.onnx").exists());
    assert!(!base.join(format!("{}.part", "m.onnx")).exists());
}

#[test]
fn installer_reports_a_failed_download() {
    let d = tmp("miss");
    let i = Installer::default();
    i.start(
        d.join("out"),
        Manifest {
            name: "x".into(),
            model_file: "x".into(),
            items: vec![Item {
                url: "file:///nonexistent/x".into(),
                sha256: "0".repeat(64),
                size: 10,
                file: "x".into(),
                extract: false,
            }],
        },
    );
    assert!(matches!(wait_done(&i), Download::Failed(_)));
    assert!(!voice::installed(&d.join("out")));
}

#[test]
fn manifest_validation_rejects_unsafe_lists() {
    let good = Item {
        url: "https://example.com/a".into(),
        sha256: "a".repeat(64),
        size: 1,
        file: "a.onnx".into(),
        extract: false,
    };
    let ok = Manifest {
        name: "v".into(),
        model_file: "a.onnx".into(),
        items: vec![good.clone()],
    };
    assert!(ok.validate().is_ok());
    let mut m = ok.clone();
    m.items[0].url = "http://example.com/a".into();
    assert!(m.validate().is_err(), "plain http is refused");
    let mut m = ok.clone();
    m.items[0].file = "../evil".into();
    m.model_file = "../evil".into();
    assert!(m.validate().is_err(), "path tricks are refused");
    let mut m = ok.clone();
    m.items[0].sha256 = "xyz".into();
    assert!(m.validate().is_err());
    let mut m = ok.clone();
    m.model_file = "other".into();
    assert!(m.validate().is_err());
    assert!(
        Manifest {
            name: "v".into(),
            model_file: "a".into(),
            items: vec![]
        }
        .validate()
        .is_err()
    );
    assert_eq!(voice::length_scale(1.0), 1.0);
    assert_eq!(voice::length_scale(1.25), 0.8);
    assert_eq!(voice::length_scale(0.75), 1.33);
}

/// Real download of the program and voice, then real synthesis. Needs the network and about 90 MB,
/// so it only runs on request with a manifest of your own:
/// RV_TEST_MANIFEST=/path/manifest.json cargo test --test voice -- --ignored
#[cfg(unix)]
#[test]
#[ignore]
fn real_download_then_real_synthesis() {
    let d = tmp("real");
    let base = d.join("voice");
    let i = Installer::default();
    let path = std::env::var("RV_TEST_MANIFEST").expect("set RV_TEST_MANIFEST");
    let m = voice::load_manifest(Path::new(&path)).expect("valid manifest");
    i.start(base.clone(), m);
    assert_eq!(wait_done(&i), Download::Done);
    // A stand-in audio player keeps the wav so the test can look at the audio itself.
    let bin = d.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let keep = d.join("kept.wav");
    std::fs::write(
        bin.join("paplay"),
        format!("#!/bin/sh\ncp \"$1\" {}\n", keep.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(bin.join("paplay"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let old = std::env::var("PATH").unwrap();
    unsafe { std::env::set_var("PATH", format!("{}:{old}", bin.display())) };
    let mut v = voice::PiperVoice::open(&base).expect("installed voice opens");
    v.speak("Hello, this is a test of the reply voice.", 1.0)
        .unwrap();
    let bytes = std::fs::read(&keep).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    let samples: Vec<i16> = bytes[44..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    let secs = samples.len() as f32 / 22050.0;
    let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
    assert!(secs > 2.0 && secs < 6.0, "unexpected length {secs}");
    assert!(peak > 3000, "audio is silent, peak {peak}");
    // Faster speed gives a shorter clip.
    v.speak("Hello, this is a test of the reply voice.", 1.5)
        .unwrap();
    let fast = (std::fs::read(&keep).unwrap().len() - 44) as f32 / 2.0 / 22050.0;
    assert!(fast < secs * 0.85, "{fast} vs {secs}");
}
