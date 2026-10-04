//! Dependency-free, host-portable helpers for the desktop's compile-time identity.
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
};

// Watch directories, rather than only their current files: Cargo will then also
// notice newly added/deleted source and asset files. Never watch a package root,
// which could recursively include Cargo's own output and cause endless rebuilds.
pub const SOURCE_TREES: &[&str] = &[
    "apps/desktop/src",
    "apps/desktop/assets",
    "apps/desktop/tests",
    "crates/core/src",
    "crates/core/examples",
    "crates/core/tests",
    "examples",
    "vendor/grid/src",
];
pub const INPUT_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".prototools",
    "rust-toolchain",
    "rust-toolchain.toml",
    ".cargo/config",
    ".cargo/config.toml",
    ".moon/toolchains.yml",
    ".moon/workspace.yml",
    "moon.yml",
    "apps/desktop/Cargo.toml",
    "apps/desktop/moon.yml",
    "apps/desktop/build.rs",
    "apps/desktop/build_support.rs",
    "crates/core/Cargo.toml",
    "crates/core/moon.yml",
    "crates/core/build.rs",
    "vendor/grid/Cargo.toml",
    "vendor/grid/build.rs",
];

#[derive(Debug)]
pub struct Inputs {
    pub watched: Vec<PathBuf>,
    pub files: BTreeMap<String, PathBuf>,
}

pub fn collect_inputs(root: &Path) -> io::Result<Inputs> {
    let mut result = Inputs {
        watched: Vec::new(),
        files: BTreeMap::new(),
    };
    for relative in SOURCE_TREES.iter().chain(INPUT_FILES) {
        let path = root.join(relative);
        // Missing optional paths must not be emitted: Cargo considers a missing
        // watched path changed on every invocation. Cargo itself tracks newly
        // introduced compiler configuration; source additions are in watched trees.
        if path.try_exists()? {
            result.watched.push(path.clone());
            collect_files(root, &path, &mut result.files)?;
        }
    }
    Ok(result)
}

fn excluded(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        matches!(
            name,
            "target" | ".git" | "node_modules" | ".cache" | ".DS_Store" | "__pycache__"
        ) || name.ends_with(".pyc")
            || name.ends_with(".pyo")
    })
}

fn collect_files(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    if fs::metadata(path)?.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !excluded(&entry.file_name()) {
                if entry.file_type()?.is_symlink() && fs::metadata(entry.path())?.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "source directory symlinks are not supported by build identity",
                    ));
                }
                collect_files(root, &entry.path(), files)?;
            }
        }
    } else {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        let name = relative
            .components()
            .map(|part| {
                part.as_os_str().to_str().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 source path")
                })
            })
            .collect::<io::Result<Vec<_>>>()?
            .join("/");
        files.insert(name, path.to_owned());
    }
    Ok(())
}

// Fixed FNV-1a 64-bit, with length-delimited paths and contents in sorted order.
// This identifies ordinary source changes; it is NOT a cryptographic signature.
const OFFSET: u64 = 0xcbf29ce484222325;
const PRIME: u64 = 0x100000001b3;
fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(PRIME);
    }
}

pub fn fingerprint(inputs: &Inputs) -> io::Result<String> {
    let mut hash = OFFSET;
    hash_bytes(&mut hash, b"libre-effects-source-v1\0");
    for (name, path) in &inputs.files {
        hash_bytes(&mut hash, &(name.len() as u64).to_le_bytes());
        hash_bytes(&mut hash, name.as_bytes());
        let mut file = fs::File::open(path)?;
        hash_bytes(&mut hash, &file.metadata()?.len().to_le_bytes());
        let mut buffer = [0; 16 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash_bytes(&mut hash, &buffer[..count]);
        }
    }
    Ok(format!("{hash:016x}"))
}

/// UTC date conversion without external `date` tools or timezone dependencies.
pub fn timestamp(unix_seconds: u64) -> (String, String) {
    let days = (unix_seconds / 86_400) as i64;
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_shifted + 2) / 5 + 1;
    let month = month_shifted + if month_shifted < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let seconds = unix_seconds % 86_400;
    let hour = seconds / 3_600;
    let minute = seconds % 3_600 / 60;
    let second = seconds % 60;
    (
        format!("{year:04}{month:02}{day:02}.{hour:02}{minute:02}{second:02}"),
        format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC"),
    )
}

pub fn build_number(unix_seconds: u64, fingerprint: &str) -> String {
    format!("{}-{fingerprint}", timestamp(unix_seconds).0)
}

#[derive(Debug)]
pub struct GitInfo {
    pub description: String,
    pub watched: Vec<PathBuf>,
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn source_description(revision: Option<&str>, dirty: Option<bool>) -> String {
    match revision {
        Some(revision) => format!(
            "Git {} ({})",
            &revision[..revision.len().min(12)],
            match dirty {
                Some(true) => "modified sources",
                Some(false) => "clean sources",
                None => "source state unavailable",
            }
        ),
        None => "Source archive / Git unavailable".into(),
    }
}

pub fn git_info(root: &Path) -> GitInfo {
    // Do not accidentally identify an enclosing repository as a source archive's
    // own history. Both a normal .git directory and worktree .git file are valid.
    if !root.join(".git").exists() {
        return GitInfo {
            description: source_description(None, None),
            watched: Vec::new(),
        };
    }
    let revision = git(root, &["rev-parse", "--verify", "HEAD"]).filter(|value| {
        matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
    });
    let mut args = vec!["status", "--porcelain=v1", "--untracked-files=all", "--"];
    args.extend(SOURCE_TREES.iter().copied());
    args.extend(INPUT_FILES.iter().copied());
    let dirty = git(root, &args).map(|output| !output.is_empty());
    let mut watched = Vec::new();
    let reference = git(root, &["symbolic-ref", "--quiet", "HEAD"]);
    for name in [
        Some("HEAD"),
        Some("index"),
        Some("packed-refs"),
        reference.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(value) = git(root, &["rev-parse", "--git-path", name]) {
            let path = root.join(value);
            if path.exists() {
                watched.push(path);
            }
        }
    }
    GitInfo {
        description: source_description(revision.as_deref(), dirty),
        watched,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "libre-effects-build-identity-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn write(&self, relative: &str, bytes: &[u8]) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        fn fingerprint(&self) -> String {
            fingerprint(&collect_inputs(&self.0).unwrap()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn timestamp_and_build_number_are_readable_utc() {
        assert_eq!(
            timestamp(0),
            ("19700101.000000".into(), "1970-01-01 00:00:00 UTC".into())
        );
        assert_eq!(
            timestamp(951_827_696),
            ("20000229.123456".into(), "2000-02-29 12:34:56 UTC".into())
        );
        assert_eq!(timestamp(4_107_542_400).1, "2100-03-01 00:00:00 UTC");
        assert_eq!(
            build_number(0, "0123456789abcdef"),
            "19700101.000000-0123456789abcdef"
        );
        assert_ne!(build_number(0, "a"), build_number(0, "b"));
        assert_ne!(build_number(0, "a"), build_number(1, "a"));
    }

    #[test]
    fn fingerprint_uses_the_documented_fnv1a_algorithm() {
        let mut hash = OFFSET;
        hash_bytes(&mut hash, b"hello");
        assert_eq!(hash, 0xa430d84680aabd0b);
    }

    #[test]
    fn fingerprint_is_stable_across_roots_and_creation_order() {
        let a = Fixture::new();
        let b = Fixture::new();
        for (path, bytes) in [
            ("apps/desktop/src/z.rs", b"last" as &[u8]),
            ("crates/core/src/a.rs", b"first"),
        ] {
            a.write(path, bytes);
        }
        b.write("crates/core/src/a.rs", b"first");
        b.write("apps/desktop/src/z.rs", b"last");
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.fingerprint().len(), 16);
        assert!(a.fingerprint().bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn uncommitted_edits_additions_deletions_and_renames_change_identity() {
        let fixture = Fixture::new();
        fixture.write("crates/core/src/geometry.rs", b"old");
        let original = fixture.fingerprint();
        fixture.write("crates/core/src/geometry.rs", b"new");
        let modified = fixture.fingerprint();
        assert_ne!(original, modified);
        fixture.write("apps/desktop/src/new.rs", b"new");
        let added = fixture.fingerprint();
        assert_ne!(modified, added);
        fs::rename(
            fixture.0.join("apps/desktop/src/new.rs"),
            fixture.0.join("apps/desktop/src/renamed.rs"),
        )
        .unwrap();
        assert_ne!(added, fixture.fingerprint());
        fs::remove_file(fixture.0.join("apps/desktop/src/renamed.rs")).unwrap();
        assert_eq!(modified, fixture.fingerprint());
    }

    #[test]
    fn watches_sources_assets_manifests_lockfile_and_build_support_but_not_outputs() {
        let fixture = Fixture::new();
        for path in [
            "apps/desktop/src/main.rs",
            "apps/desktop/assets/icons/play.svg",
            "apps/desktop/tests/identity.rs",
            "crates/core/src/lib.rs",
            "crates/core/tests/data/fixture.json",
            "examples/project.lfe.json",
            "vendor/grid/src/lib.rs",
            "Cargo.toml",
            "Cargo.lock",
            "apps/desktop/Cargo.toml",
            "apps/desktop/build.rs",
            "apps/desktop/build_support.rs",
            "apps/desktop/moon.yml",
            "crates/core/Cargo.toml",
            "vendor/grid/Cargo.toml",
            ".cargo/config.toml",
        ] {
            fixture.write(path, b"source");
            let inputs = collect_inputs(&fixture.0).unwrap();
            assert!(inputs.files.contains_key(path), "{path}");
            assert!(
                inputs
                    .watched
                    .iter()
                    .any(|watch| fixture.0.join(path).starts_with(watch)),
                "{path}"
            );
        }
        let original = fixture.fingerprint();
        for path in [
            "target/debug/output",
            "apps/desktop/target/output",
            "vendor/grid/target/output",
            ".moon/cache/output",
            ".git/HEAD",
            "apps/web/src/page.tsx",
            "apps/desktop/src/target/generated",
            "apps/desktop/src/.DS_Store",
            "crates/core/tests/data/__pycache__/fixture.pyc",
            "crates/core/tests/data/fixture.pyc",
        ] {
            fixture.write(path, b"unrelated");
        }
        let inputs = collect_inputs(&fixture.0).unwrap();
        assert_eq!(original, fixture.fingerprint());
        assert!(!inputs.watched.contains(&fixture.0));
        assert!(!inputs.watched.contains(&fixture.0.join("apps/desktop")));
        assert!(!inputs.watched.iter().any(|path| !path.exists()));
        for path in [
            "apps/desktop/assets/icons/play.svg",
            "Cargo.toml",
            "Cargo.lock",
            "crates/core/tests/data/fixture.json",
            "examples/project.lfe.json",
            "apps/desktop/Cargo.toml",
            "apps/desktop/build.rs",
            "apps/desktop/build_support.rs",
            "crates/core/Cargo.toml",
            "vendor/grid/Cargo.toml",
            "vendor/grid/src/lib.rs",
            ".cargo/config.toml",
        ] {
            fixture.write(path, b"changed");
            assert_ne!(original, fixture.fingerprint(), "{path}");
            fixture.write(path, b"source");
            assert_eq!(original, fixture.fingerprint(), "{path}");
        }
    }

    #[test]
    fn archive_fallback_does_not_require_git_or_claim_clean_sources() {
        let fixture = Fixture::new();
        fixture.write("apps/desktop/src/main.rs", b"source archive");
        let info = git_info(&fixture.0);
        assert_eq!(info.description, "Source archive / Git unavailable");
        assert!(info.watched.is_empty());
        let revision = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            source_description(Some(revision), Some(true)),
            "Git 0123456789ab (modified sources)"
        );
        assert_eq!(
            source_description(Some(revision), Some(false)),
            "Git 0123456789ab (clean sources)"
        );
        assert_eq!(
            source_description(Some(revision), None),
            "Git 0123456789ab (source state unavailable)"
        );
    }
}
