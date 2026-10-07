mod build_support;

use std::{
    env,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    let root = manifest
        .parent()
        .and_then(|path| path.parent())
        .expect("desktop workspace root");
    let inputs = build_support::collect_inputs(root).expect("read build identity source inputs");
    for path in &inputs.watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let source = build_support::fingerprint(&inputs).expect("fingerprint build identity sources");
    let git = build_support::git_info(root);
    for path in git.watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("build time after Unix epoch")
        .as_secs();
    // Emit metadata only to Cargo. Never write a source-tree counter or read Git
    // at application runtime. OUT_DIR and all target outputs are not source inputs.
    println!(
        "cargo:rustc-env=LIBRE_EFFECTS_BUILD_NUMBER={}",
        build_support::build_number(now, &source)
    );
    println!(
        "cargo:rustc-env=LIBRE_EFFECTS_BUILD_UTC={}",
        build_support::timestamp(now).1
    );
    println!("cargo:rustc-env=LIBRE_EFFECTS_SOURCE_FINGERPRINT={source}");
    println!(
        "cargo:rustc-env=LIBRE_EFFECTS_SOURCE_DESCRIPTION={}",
        git.description
    );
    println!(
        "cargo:rustc-env=LIBRE_EFFECTS_BUILD_TARGET={}",
        env::var("TARGET").expect("Cargo build target")
    );
    println!(
        "cargo:rustc-env=LIBRE_EFFECTS_BUILD_PROFILE={}",
        env::var("PROFILE").expect("Cargo build profile")
    );
}
