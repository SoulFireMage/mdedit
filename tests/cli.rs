//! End-to-end checks of the headless command-line modes (no display needed).

use std::path::PathBuf;
use std::process::Command;

fn mdedit() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mdedit"))
}

fn fixture(name: &str, contents: &[u8]) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn render_prints_html_fragment() {
    let path = fixture(
        "render.md",
        b"# Hi\r\n\r\n| a | b |\r\n|---|---|\r\n| 1 | 2 |\r\n",
    );
    let out = mdedit().arg("--render").arg(&path).output().unwrap();
    assert!(out.status.success());
    let html = String::from_utf8(out.stdout).unwrap();
    assert!(html.starts_with("<h1>Hi</h1>\n"), "got: {html}");
    assert!(html.contains("<td>1</td>"), "got: {html}");
    assert!(!html.contains("data-sourcepos"), "got: {html}");
}

#[test]
fn render_reports_missing_file() {
    let out = mdedit()
        .args(["--render", "/nonexistent/mdedit-test.md"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Cannot read"));
}

#[test]
fn render_rejects_non_utf8() {
    let path = fixture("bad.md", &[0x66, 0xff]);
    let out = mdedit().arg("--render").arg(&path).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not valid UTF-8"));
}

#[test]
fn render_without_file_is_usage_error() {
    let out = mdedit().arg("--render").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn version_prints_version() {
    let out = mdedit().arg("--version").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.trim(), format!("mdedit {}", env!("CARGO_PKG_VERSION")));
}
