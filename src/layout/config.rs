use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::layout::{expand_home, pane_command, substitute, Environment};

/// Extensions of layout files, in the order they are looked up.
const EXTENSIONS: [&str; 2] = ["yml", "yaml"];

/// Layout declares a tmux session and its windows.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// The session the windows belong to.
    pub session: Session,
    /// The windows, in order.
    pub windows: Vec<Window>,
}

/// Session is the tmux session a layout creates.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Session {
    /// Name of the session.
    pub name: Option<String>,
    /// Working directory of every pane, unless its window or the pane sets one.
    pub cwd: Option<String>,
}

/// Window is a tmux window and its panes.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Window {
    /// Name of the window.
    pub name: Option<String>,
    /// Layout passed to `tmux select-layout`, like `tiled` or `main-vertical`.
    pub layout: Option<String>,
    /// Working directory of every pane, unless the pane sets one.
    pub cwd: Option<String>,
    /// The panes, in order. The first one is created with the window.
    pub panes: Vec<Pane>,
}

/// Pane is a tmux pane.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Pane {
    /// Title of the pane.
    pub name: Option<String>,
    /// Command run in the pane, instead of the default shell.
    pub command: Option<String>,
    /// Working directory of the pane.
    pub cwd: Option<String>,
}

/// WindowPlan is a window ready to be created: names and paths resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowPlan {
    /// Name of the window.
    pub name: Option<String>,
    /// Layout passed to `tmux select-layout`.
    pub layout: Option<String>,
    /// The panes; never empty.
    pub panes: Vec<PanePlan>,
}

/// PanePlan is a pane ready to be created: its working directory and command resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanePlan {
    /// Title of the pane.
    pub title: Option<String>,
    /// Working directory of the pane.
    pub cwd: Option<String>,
    /// Shell command tmux runs in the pane.
    pub command: Option<String>,
}

/// Returns `value`, unless it is unset or empty.
fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|v| !v.is_empty())
}

impl Layout {
    /// Reads the layout in `path`, with the variables it references replaced from `env`.
    pub fn read_from_file(path: &Path, env: &Environment) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::parse(&substitute(&text, env))
            .with_context(|| format!("invalid layout {}", path.display()))
    }

    /// Parses and checks a layout.
    pub fn parse(text: &str) -> Result<Self> {
        let layout: Self = serde_yaml_ng::from_str(text)?;
        if present(&layout.session.name).is_none() {
            bail!("session.name is required");
        }
        if layout.windows.is_empty() {
            bail!("at least one window must be declared");
        }
        Ok(layout)
    }

    /// Returns the name of the session.
    pub fn session_name(&self) -> &str {
        present(&self.session.name).unwrap_or_default()
    }

    /// Returns the windows to create, resolved against the session and `env`.
    pub fn plan(&self, env: &Environment) -> Vec<WindowPlan> {
        self.windows
            .iter()
            .map(|window| {
                // A window without panes still has the one it is created with
                let default = [Pane::default()];
                let panes = match window.panes.is_empty() {
                    true => &default[..],
                    false => &window.panes[..],
                };
                WindowPlan {
                    name: present(&window.name).map(str::to_string),
                    layout: present(&window.layout).map(str::to_string),
                    panes: panes
                        .iter()
                        .map(|pane| PanePlan {
                            title: present(&pane.name).map(str::to_string),
                            cwd: self.cwd(window, pane).map(|cwd| expand_home(cwd, env)),
                            command: present(&pane.command).map(|c| pane_command(c, env)),
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// Returns the working directory of `pane`: its own, or else its window's, or else the
    /// session's.
    fn cwd<'a>(&'a self, window: &'a Window, pane: &'a Pane) -> Option<&'a str> {
        present(&pane.cwd)
            .or_else(|| present(&window.cwd))
            .or_else(|| present(&self.session.cwd))
    }
}

/// Returns the file of the layout `name` in `dir`.
pub fn find(dir: &Path, name: &str) -> Result<PathBuf> {
    EXTENSIONS
        .iter()
        .map(|ext| dir.join(format!("{name}.{ext}")))
        .find(|path| path.is_file())
        .with_context(|| format!("layout '{name}' not found in {}", dir.display()))
}

/// Returns the names of the layouts in `dir`, sorted.
pub fn list(dir: &Path) -> Result<Vec<String>> {
    if !dir.is_dir() {
        bail!("layout directory does not exist: {}", dir.display());
    }

    let mut names = Vec::new();
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let path = entry?.path();
        let layout = path
            .extension()
            .is_some_and(|ext| EXTENSIONS.iter().any(|e| ext == *e));
        if layout && path.is_file() {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    if names.is_empty() {
        bail!("no layouts in {}", dir.display());
    }

    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn env(vars: &[(&str, &str)]) -> Environment {
        vars.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn pane(cwd: &str) -> PanePlan {
        PanePlan {
            cwd: Some(cwd.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn parse_reads_a_layout() {
        let layout = Layout::parse(indoc! {"
            session:
              name: my-session
            windows:
              - name: editor
                layout: tiled
                panes:
                  - name: git
                    command: tig
                  - command: nvim
        "})
        .unwrap();
        assert_eq!(layout.session_name(), "my-session");
        assert_eq!(
            layout.plan(&env(&[])),
            vec![WindowPlan {
                name: Some("editor".to_string()),
                layout: Some("tiled".to_string()),
                panes: vec![
                    PanePlan {
                        title: Some("git".to_string()),
                        cwd: None,
                        command: Some("tig".to_string()),
                    },
                    PanePlan {
                        title: None,
                        cwd: None,
                        command: Some("nvim".to_string()),
                    },
                ],
            }]
        );
    }

    #[test]
    fn parse_requires_a_session_name() {
        let err = Layout::parse("windows: [{name: w}]").unwrap_err();
        assert!(err.to_string().contains("session.name is required"));

        let err = Layout::parse("session: {name: ''}\nwindows: [{name: w}]").unwrap_err();
        assert!(err.to_string().contains("session.name is required"));
    }

    #[test]
    fn parse_requires_a_window() {
        let err = Layout::parse("session: {name: x}\nwindows: []").unwrap_err();
        assert!(err.to_string().contains("at least one window"));
    }

    #[test]
    fn plan_takes_the_most_specific_cwd() {
        let layout = Layout::parse(indoc! {"
            session: {name: x, cwd: ~/s}
            windows:
              - cwd: ~/w
                panes: [{cwd: ~/p}, {}]
              - panes: [{}]
        "})
        .unwrap();
        let windows = layout.plan(&env(&[("HOME", "/h")]));
        assert_eq!(windows[0].panes, vec![pane("/h/p"), pane("/h/w")]);
        assert_eq!(windows[1].panes, vec![pane("/h/s")]);
    }

    #[test]
    fn plan_treats_empty_and_null_values_as_unset() {
        let layout = Layout::parse(indoc! {"
            session: {name: x, cwd: /s}
            windows:
              - name: ''
                layout: null
                cwd: ''
                panes: [{name: '', command: '', cwd: null}]
        "})
        .unwrap();
        assert_eq!(
            layout.plan(&env(&[])),
            vec![WindowPlan {
                name: None,
                layout: None,
                panes: vec![pane("/s")],
            }]
        );
    }

    #[test]
    fn plan_gives_a_window_without_panes_one() {
        let layout = Layout::parse("session: {name: x}\nwindows: [{name: w, cwd: /w}]").unwrap();
        assert_eq!(layout.plan(&env(&[]))[0].panes, vec![pane("/w")]);
    }

    #[test]
    fn plan_runs_commands_in_nix_develop_inside_a_nix_shell() {
        let layout =
            Layout::parse("session: {name: x}\nwindows: [{panes: [{command: tig}]}]").unwrap();
        let windows = layout.plan(&env(&[("IN_NIX_SHELL", "impure"), ("SHELL", "/bin/sh")]));
        assert_eq!(
            windows[0].panes[0].command.as_deref(),
            Some("nix develop -c /bin/sh -c tig")
        );
    }

    #[test]
    fn read_from_file_substitutes_variables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dev.yml");
        std::fs::write(&path, "session: {name: \"${USER}-dev\"}\nwindows: [{}]").unwrap();
        let layout = Layout::read_from_file(&path, &env(&[("USER", "me")])).unwrap();
        assert_eq!(layout.session_name(), "me-dev");
    }

    #[test]
    fn read_from_file_names_the_file_on_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yml");
        std::fs::write(&path, "windows: [{}]").unwrap();
        let err = Layout::read_from_file(&path, &env(&[])).unwrap_err();
        assert!(format!("{err:#}").contains(&path.display().to_string()));
    }

    #[test]
    fn find_prefers_yml_over_yaml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("dev.yaml"), "").unwrap();
        assert_eq!(
            find(dir.path(), "dev").unwrap(),
            dir.path().join("dev.yaml")
        );

        std::fs::write(dir.path().join("dev.yml"), "").unwrap();
        assert_eq!(find(dir.path(), "dev").unwrap(), dir.path().join("dev.yml"));
    }

    #[test]
    fn find_fails_for_a_missing_layout() {
        let dir = tempfile::tempdir().unwrap();
        let err = find(dir.path(), "nope").unwrap_err();
        assert!(err.to_string().contains("layout 'nope' not found"));
    }

    #[test]
    fn list_returns_sorted_names_without_extensions() {
        let dir = tempfile::tempdir().unwrap();
        for file in ["work.yml", "dev.yaml", "dev.yml", "notes.txt"] {
            std::fs::write(dir.path().join(file), "").unwrap();
        }
        std::fs::create_dir(dir.path().join("dir.yml")).unwrap();
        assert_eq!(list(dir.path()).unwrap(), vec!["dev", "work"]);
    }

    #[test]
    fn list_fails_without_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let err = list(dir.path()).unwrap_err();
        assert!(err.to_string().contains("no layouts"));

        let err = list(&dir.path().join("missing")).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }
}
