//! The development bypass must not survive a release build.
//!
//! A flag that can be set is a flag that will be set, at three in the morning,
//! to make something work. So the bypass is a compile-time feature and the
//! compiler refuses it in release, which is a claim worth checking rather than
//! reasoning about: `cfg` conditions are easy to get subtly inverted, and the
//! failure mode is a production binary that accepts unsigned requests.

use std::process::Command;

#[test]
#[ignore = "compiles the crate twice; run with --ignored or in CI"]
fn a_release_build_refuses_the_development_bypass() {
    let output = Command::new(env!("CARGO"))
        .args([
            "check",
            "-p",
            "hlin-identity",
            "--features",
            "dev-identity",
            "--release",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");

    assert!(
        !output.status.success(),
        "a release build with `dev-identity` must not compile"
    );

    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("must never be compiled into a release build"),
        "the refusal should say why, got: {complaint}"
    );
}

#[test]
#[ignore = "compiles the crate twice; run with --ignored or in CI"]
fn a_debug_build_allows_it_so_platform_teams_can_work() {
    let output = Command::new(env!("CARGO"))
        .args(["check", "-p", "hlin-identity", "--features", "dev-identity"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");

    assert!(
        output.status.success(),
        "a debug build with `dev-identity` should compile: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
