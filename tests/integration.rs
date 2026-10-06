use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Workspace holds a layout directory, a home directory and the socket directory of a tmux
/// server of its own, which is killed when the workspace is dropped.
struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        for sub in ["layouts", "home", "tmux"] {
            std::fs::create_dir(dir.path().join(sub)).unwrap();
        }
        Self { dir }
    }

    fn path(&self, sub: &str) -> PathBuf {
        // tmux reports the canonical form of paths, like /private/var on macOS
        self.dir.path().canonicalize().unwrap().join(sub)
    }

    /// Writes the layout `name`.
    fn layout(self, name: &str, body: &str) -> Self {
        std::fs::write(self.path("layouts").join(format!("{name}.yml")), body).unwrap();
        self
    }

    /// Returns a tmux-layout command using this workspace, outside tmux and a Nix shell.
    fn tmux_layout(&self) -> assert_cmd::Command {
        let mut cmd = cargo_bin_cmd!();
        cmd.env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env_remove("IN_NIX_SHELL")
            .env("HOME", self.path("home"))
            .env("TMUX_LAYOUT_DIR", self.path("layouts"))
            .env("TMUX_TMPDIR", self.path("tmux"));
        cmd
    }

    /// Runs tmux against the server of this workspace and returns its output lines.
    fn tmux(&self, args: &[&str]) -> Vec<String> {
        let output = self.tmux_command(args).output().unwrap();
        assert!(output.status.success(), "tmux {args:?} failed");
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Returns a tmux command for the server of this workspace. Never the server of the tmux
    /// the tests run in: `-L` makes tmux ignore $TMUX, which would otherwise win over
    /// $TMUX_TMPDIR.
    fn tmux_command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("tmux");
        cmd.args(["-L", "default"])
            .args(args)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env("TMUX_TMPDIR", self.path("tmux"));
        cmd
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = self.tmux_command(&["kill-server"]).output();
    }
}

#[test]
fn help_lists_subcommands() {
    cargo_bin_cmd!()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("switch").and(predicate::str::contains("list")));
}

#[test]
fn no_arguments_prints_help() {
    cargo_bin_cmd!()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn version_prints_the_package_version() {
    cargo_bin_cmd!()
        .arg("-V")
        .assert()
        .success()
        .stdout(format!("tmux-layout {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn unknown_subcommand_fails() {
    cargo_bin_cmd!()
        .arg("bogus")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand 'bogus'"));
}

#[test]
fn list_prints_layout_names() {
    let ws = Workspace::new()
        .layout("work", "session: {name: y}")
        .layout("dev", "session: {name: x}");
    ws.tmux_layout()
        .arg("list")
        .assert()
        .success()
        .stdout("dev\nwork\n");
}

#[test]
fn list_uses_the_xdg_config_home() {
    let ws = Workspace::new();
    let layouts = ws.path("config/tmux/layouts");
    std::fs::create_dir_all(&layouts).unwrap();
    std::fs::write(layouts.join("dev.yaml"), "").unwrap();
    ws.tmux_layout()
        .env_remove("TMUX_LAYOUT_DIR")
        .env("XDG_CONFIG_HOME", ws.path("config"))
        .arg("list")
        .assert()
        .success()
        .stdout("dev\n");
}

#[test]
fn list_fails_without_layouts() {
    let ws = Workspace::new();
    ws.tmux_layout()
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no layouts"));
}

#[test]
fn switch_fails_for_a_missing_layout() {
    let ws = Workspace::new();
    ws.tmux_layout()
        .args(["switch", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("layout 'nope' not found"));
}

#[test]
fn switch_fails_for_an_invalid_layout() {
    let ws = Workspace::new().layout("bad", "windows: [{name: w}]");
    ws.tmux_layout()
        .args(["switch", "bad"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("session.name is required"));
}

// The tests below run a tmux server. Attaching fails without a terminal, so they check the
// session after `switch` ran instead of its exit status.

/// Asserts that `pane` in the server of `ws` runs in `expected`. On Linux, tmux reads the
/// directory of the process in the pane, which may not have changed to it yet right after the
/// pane was created, so this waits for it for a while.
fn assert_pane_path(ws: &Workspace, pane: &str, expected: &Path) {
    let mut path = PathBuf::new();
    for _ in 0..50 {
        let lines = ws.tmux(&["display-message", "-t", pane, "-p", "#{pane_current_path}"]);
        path = PathBuf::from(&lines[0]);
        if path == expected {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(path, expected, "working directory of {pane}");
}

#[test]
fn switch_creates_a_session() {
    let ws = Workspace::new().layout(
        "demo",
        "session:
  name: t-demo
windows:
  - name: editor
    panes:
      - name: first
        command: sleep 100
      - command: sleep 100
  - name: notes
    panes:
      - command: sleep 100
",
    );
    ws.tmux_layout().args(["switch", "demo"]).output().unwrap();

    let windows = ws.tmux(&["list-windows", "-t", "=t-demo", "-F", "#{window_name}"]);
    assert_eq!(windows, vec!["editor", "notes"]);
    let panes = ws.tmux(&["list-panes", "-t", "=t-demo:editor", "-F", "#{pane_title}"]);
    assert_eq!(panes.len(), 2);
    assert_eq!(panes[0], "first");
}

#[test]
fn switch_attaches_to_an_existing_session() {
    let ws = Workspace::new().layout(
        "demo",
        "session: {name: t-idem}\nwindows: [{name: only, panes: [{command: sleep 100}]}]",
    );
    ws.tmux_layout().args(["switch", "demo"]).output().unwrap();
    ws.tmux_layout()
        .args(["switch", "demo"])
        .assert()
        .stderr(predicate::str::contains("already exists"));

    let windows = ws.tmux(&["list-windows", "-t", "=t-idem", "-F", "#{window_name}"]);
    assert_eq!(windows, vec!["only"]);
}

#[test]
fn switch_substitutes_environment_variables() {
    let ws = Workspace::new().layout(
        "env",
        "session: {name: \"t-${SUFFIX}\"}\nwindows: [{panes: [{command: sleep 100}]}]",
    );
    ws.tmux_layout()
        .env("SUFFIX", "substituted")
        .args(["switch", "env"])
        .output()
        .unwrap();

    ws.tmux(&["has-session", "-t", "=t-substituted"]);
}

#[test]
fn switch_applies_the_most_specific_cwd() {
    let ws = Workspace::new();
    let (s, w, p) = (ws.path("home/s"), ws.path("w"), ws.path("p"));
    for dir in [&s, &w, &p] {
        std::fs::create_dir(dir).unwrap();
    }
    let ws = ws.layout(
        "cwd",
        &format!(
            "session:
  name: t-cwd
  cwd: ~/s
windows:
  - name: session
    panes:
      - command: sleep 100
  - name: window
    cwd: {}
    panes:
      - command: sleep 100
      - command: sleep 100
        cwd: {}
",
            w.display(),
            p.display()
        ),
    );
    ws.tmux_layout().args(["switch", "cwd"]).output().unwrap();

    assert_pane_path(&ws, "=t-cwd:session.0", &s);
    assert_pane_path(&ws, "=t-cwd:window.0", &w);
    assert_pane_path(&ws, "=t-cwd:window.1", &p);
}

#[test]
fn switch_adds_windows_to_the_current_session_inside_tmux() {
    let ws = Workspace::new().layout(
        "demo",
        "session: {name: ignored}\nwindows: [{name: added, panes: [{command: sleep 100}]}]",
    );
    ws.tmux(&[
        "new-session",
        "-d",
        "-s",
        "current",
        "-n",
        "first",
        "sleep 100",
    ]);

    // Run inside the current session, as tmux would run it from one of its panes
    let socket = ws.tmux(&["display-message", "-t", "=current", "-p", "#{socket_path}"]);
    let pane = ws.tmux(&["display-message", "-t", "=current", "-p", "#{pane_id}"]);
    ws.tmux_layout()
        .env("TMUX", format!("{},0,0", socket[0]))
        .env("TMUX_PANE", &pane[0])
        .args(["switch", "demo"])
        .assert()
        .success();

    let windows = ws.tmux(&["list-windows", "-t", "=current", "-F", "#{window_name}"]);
    assert_eq!(windows, vec!["first", "added"]);
    let sessions = ws.tmux(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(sessions, vec!["current"]);
}
