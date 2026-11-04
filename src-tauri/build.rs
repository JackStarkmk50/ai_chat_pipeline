fn main() {
    // Linux: find libtranscribe + the dlopen'd ggml backend modules next to
    // the binary in the package. deb/rpm install them into the app-private
    // "/usr/lib/Reply Voice" (the dir tauri uses for resources), the AppImage
    // keeps them in usr/lib, and plain cargo runs keep them beside the exe.
    // Windows resolves DLLs from the exe directory, so it needs no rpath.
    // (Handy src-tauri/build.rs, MIT, (c) 2025 CJ Pais.)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/Reply Voice:$ORIGIN/../lib:$ORIGIN");
    }

    stage_transcribe_runtime_libs();

    tauri_build::build()
}

/// Stage transcribe-cpp's shared runtime libraries (and the dlopen'd ggml
/// backend modules) for the bundler. Self-gates on the shared /
/// dynamic-backends posture used by Linux and Windows; a static build has
/// nothing to ship. Adopted from Handy's build.rs (MIT, (c) 2025 CJ Pais);
/// the VC-redist staging there is CI-installer hardening we do not need yet.
fn stage_transcribe_runtime_libs() {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=DEP_TRANSCRIBE_CPP_RUNTIME_DIR");
    println!("cargo:rerun-if-env-changed=DEP_TRANSCRIBE_CPP_MODULE_DIR");

    // Present only in a shared posture. A static build has nothing to ship.
    let Some(runtime_dir) = std::env::var_os("DEP_TRANSCRIBE_CPP_RUNTIME_DIR") else {
        return;
    };

    // RUNTIME_DIR: the shared libs to load (transcribe + core ggml/ggml-base).
    // MODULE_DIR: the dlopen'd ggml backend modules (per-ISA ggml-cpu-* and
    // ggml-vulkan), dynamic-backends only. BOTH must sit next to the
    // executable or init_backends_default() registers zero compute devices.
    let mut dirs = BTreeSet::new();
    dirs.insert(PathBuf::from(runtime_dir));
    if let Some(module_dir) = std::env::var_os("DEP_TRANSCRIBE_CPP_MODULE_DIR") {
        dirs.insert(PathBuf::from(module_dir));
    }

    let dest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("transcribe-libs");
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).expect("create transcribe-libs staging dir");

    // Match by NAME, not extension: Linux versions its libs (libtranscribe.so.0)
    // and the loader needs the SONAME.
    let mut libs: std::collections::BTreeMap<String, PathBuf> = Default::default();
    for dir in &dirs {
        println!("cargo:rerun-if-changed={}", dir.display());
        for entry in std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
        {
            let src = entry.path();
            let name = src.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let is_lib = name.ends_with(".dll")
                || name.ends_with(".dylib")
                || name.ends_with(".so")
                || name.contains(".so.");
            if is_lib {
                libs.insert(name.to_string(), src);
            }
        }
    }

    // Tauri's deb/rpm bundlers flatten symlinks into real files, so stage only
    // one name per symlink family: the shortest versioned name is the SONAME
    // for linked core libs; backend modules have only their bare name.
    let mut best: std::collections::BTreeMap<&str, (&str, &PathBuf, usize)> = Default::default();
    for (name, src) in &libs {
        let (stem, rank) = match split_versioned_so(name) {
            None => (name.as_str(), 0),
            Some((stem, 0)) => (stem, usize::MAX),
            Some((stem, depth)) => (stem, depth - 1),
        };
        match best.get(stem) {
            Some(&(_, _, existing)) if existing <= rank => {}
            _ => {
                best.insert(stem, (name, src, rank));
            }
        }
    }

    let mut copied = 0usize;
    for &(name, src, _) in best.values() {
        std::fs::copy(src, dest.join(name))
            .unwrap_or_else(|e| panic!("copy {}: {e}", src.display()));
        copied += 1;
    }
    if copied == 0 {
        panic!(
            "no transcribe-cpp runtime libraries found under {dirs:?}; a shared / \
             dynamic-backends build must ship them or the app registers zero \
             compute devices"
        );
    }
    println!("cargo:warning=Staged {copied} transcribe-cpp runtime library file(s)");
}

/// Split a versioned ELF shared-library name into (stem, version depth):
/// `libfoo.so` -> ("libfoo", 0), `libfoo.so.0` -> ("libfoo", 1).
/// (Handy build.rs, MIT.)
fn split_versioned_so(name: &str) -> Option<(&str, usize)> {
    let idx = name.find(".so")?;
    let (stem, rest) = (&name[..idx], &name[idx + 3..]);
    if rest.is_empty() {
        return Some((stem, 0));
    }
    let comps: Vec<&str> = rest.strip_prefix('.')?.split('.').collect();
    comps
        .iter()
        .all(|c| !c.is_empty() && c.bytes().all(|b| b.is_ascii_digit()))
        .then_some((stem, comps.len()))
}
