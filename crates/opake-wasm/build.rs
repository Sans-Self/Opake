// Stamps the build time and git revision into the binary so the running
// WASM can report which build it is. Primarily a diagnostic for the web app:
// when the browser serves a cached `opake_bg.wasm`, `buildInfo()` reports the
// *stale* build, making "is the browser running the rebuild?" answerable.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("cargo:rustc-env=OPAKE_BUILD_EPOCH={epoch}");

    let git = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=OPAKE_GIT_HASH={git}");

    // Re-stamp when the crate's own sources change (keeps the build time in
    // step with actual recompiles) and when HEAD moves (keeps the git hash
    // current). Emitting any `rerun-if-changed` overrides cargo's default
    // "rerun on any package change", so both triggers are listed explicitly.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
