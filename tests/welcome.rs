use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn no_args_prints_welcome_to_stdout() {
    let pkg_version = env!("CARGO_PKG_VERSION");

    Command::cargo_bin("cdx")
        .expect("binary `cdx` built by cargo")
        .assert()
        .success()
        .stdout(contains(format!("codex v{pkg_version}")))
        .stdout(contains("Terminal-first ebook library"))
        .stdout(contains("cdx --help"));
}

#[test]
fn help_flag_is_handled_by_clap() {
    Command::cargo_bin("cdx")
        .expect("binary `cdx` built by cargo")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage:"));
}

#[test]
fn version_flag_is_handled_by_clap() {
    let pkg_version = env!("CARGO_PKG_VERSION");

    Command::cargo_bin("cdx")
        .expect("binary `cdx` built by cargo")
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(pkg_version));
}
