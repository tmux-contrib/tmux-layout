use anyhow::{bail, Result};
use std::path::Path;

use crate::layout::{quote, tilde, unwrap_pane_command, words, Environment};
use crate::layout::{Layout, Pane, Session, Window};

/// Separates the fields of [`PANE_FORMAT`]. tmux prints it as is, and names, titles and paths
/// don't hold it.
const SEPARATOR: char = '\x1f';

/// Format of the line `tmux list-panes` prints for each pane, read by [`Layout::capture`].
pub const PANE_FORMAT: &str = concat!(
    "#{session_name}\x1f#{window_id}\x1f#{window_name}\x1f#{automatic-rename}\x1f",
    "#{window_layout}\x1f#{pane_current_path}\x1f#{pane_start_command}\x1f",
    "#{pane_current_command}\x1f#{pane_title}\x1f#{host}"
);

/// Programs that are shells: a pane running one of them runs no command of its own.
const SHELLS: [&str; 15] = [
    "ash", "bash", "csh", "dash", "elvish", "fish", "ksh", "mksh", "nu", "pwsh", "sh", "tcsh",
    "xonsh", "yash", "zsh",
];

/// PaneInfo is a pane as `tmux list-panes` prints it with [`PANE_FORMAT`].
struct PaneInfo<'a> {
    session_name: &'a str,
    window_id: &'a str,
    window_name: &'a str,
    automatic_rename: bool,
    window_layout: &'a str,
    /// Working directory, with the home directory written as `~`.
    cwd: Option<String>,
    start_command: &'a str,
    current_command: &'a str,
    title: &'a str,
    host: &'a str,
}

impl<'a> PaneInfo<'a> {
    /// Parses a line printed with [`PANE_FORMAT`].
    fn parse(line: &'a str, env: &Environment) -> Result<Self> {
        let fields: Vec<&str> = line.split(SEPARATOR).collect();
        let [session_name, window_id, window_name, automatic_rename, window_layout, path, start_command, current_command, title, host] =
            fields[..]
        else {
            bail!("unexpected output from tmux: {line}");
        };
        Ok(Self {
            session_name,
            window_id,
            window_name,
            automatic_rename: automatic_rename == "1",
            window_layout,
            cwd: (!path.is_empty()).then(|| tilde(Path::new(path), env)),
            start_command,
            current_command,
            title,
            host,
        })
    }

    /// Returns the title of the pane, unless it is the default one: the host name.
    fn name(&self) -> Option<String> {
        let title = self.title;
        (!title.is_empty() && title != self.host).then(|| title.to_string())
    }

    /// Returns the command the pane was started with, or else the program it runs, unless it
    /// is a shell.
    fn command(&self) -> Option<String> {
        if !self.start_command.is_empty() {
            // tmux prints the arguments of the command escaped, joined with spaces
            let command = match &words(self.start_command)[..] {
                [command] => command.clone(),
                args => args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" "),
            };
            return Some(unwrap_pane_command(&command));
        }
        // Login shells start with a dash, like -zsh
        let program = self.current_command.trim_start_matches('-');
        (!program.is_empty() && !SHELLS.contains(&program)).then(|| program.to_string())
    }
}

impl Layout {
    /// Returns the layout of the session `tmux list-panes -s` printed in `output` with
    /// [`PANE_FORMAT`]. The working directory most panes share is set on the session, and the
    /// one most panes of a window share on the window when it differs, so panes only set
    /// theirs when it differs too. Paths in the home directory in `env` are written with `~`.
    pub fn capture(output: &str, env: &Environment) -> Result<Self> {
        let panes = output
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| PaneInfo::parse(line, env))
            .collect::<Result<Vec<_>>>()?;
        let Some(first) = panes.first() else {
            bail!("tmux printed no panes");
        };

        let cwds = |panes: &'_ [PaneInfo]| -> Vec<String> {
            panes.iter().filter_map(|p| p.cwd.clone()).collect()
        };
        let session_cwd = most_common(&cwds(&panes), None);

        // tmux lists the panes window by window
        let windows = panes
            .chunk_by(|a, b| a.window_id == b.window_id)
            .map(|panes| {
                let window = &panes[0];
                let window_cwd = most_common(&cwds(panes), session_cwd.as_deref());
                let inherited = window_cwd.as_ref().or(session_cwd.as_ref());
                let mut layout_panes: Vec<Pane> = panes
                    .iter()
                    .map(|pane| Pane {
                        name: pane.name(),
                        command: pane.command(),
                        cwd: pane.cwd.clone().filter(|cwd| Some(cwd) != inherited),
                    })
                    .collect();
                // A window is created with a pane running the shell anyway
                if let [pane] = &layout_panes[..] {
                    if *pane == Pane::default() {
                        layout_panes.clear();
                    }
                }
                Window {
                    // Automatic names follow the program running, so they are left to tmux
                    name: (!window.automatic_rename && !window.window_name.is_empty())
                        .then(|| window.window_name.to_string()),
                    // The layout of a single pane is the window
                    layout: (panes.len() > 1).then(|| window.window_layout.to_string()),
                    cwd: window_cwd.filter(|cwd| Some(cwd) != session_cwd.as_ref()),
                    panes: layout_panes,
                }
            })
            .collect();

        Ok(Self {
            session: Session {
                name: Some(first.session_name.to_string()),
                cwd: session_cwd,
            },
            windows,
        })
    }
}

/// Returns the value that occurs most in `values`. Ties go to `preferred`, then to the value
/// that occurs first.
fn most_common(values: &[String], preferred: Option<&str>) -> Option<String> {
    let count = |value: &str| values.iter().filter(|v| *v == value).count();
    let mut best = preferred.filter(|p| count(p) > 0);
    for value in values {
        if best.is_none_or(|b| count(value) > count(b)) {
            best = Some(value);
        }
    }
    best.map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn env() -> Environment {
        Environment::from([("HOME".to_string(), "/h".to_string())])
    }

    /// Returns the output of `tmux list-panes` for `panes`, each of them the fields of
    /// [`PANE_FORMAT`] separated with `|`.
    fn output(panes: &[&str]) -> String {
        panes
            .iter()
            .map(|pane| pane.replace('|', "\x1f") + "\n")
            .collect()
    }

    fn capture(panes: &[&str]) -> Layout {
        Layout::capture(&output(panes), &env()).unwrap()
    }

    fn pane(name: Option<&str>, command: Option<&str>, cwd: Option<&str>) -> Pane {
        Pane {
            name: name.map(str::to_string),
            command: command.map(str::to_string),
            cwd: cwd.map(str::to_string),
        }
    }

    #[test]
    fn capture_reads_a_session() {
        let layout = capture(&[
            "dev|@1|editor|0|c195,80x24,0,0[80x12,0,0,0,80x11,0,13,1]|/h/app||zsh|host|host",
            "dev|@1|editor|0|c195,80x24,0,0[80x12,0,0,0,80x11,0,13,1]|/h/app|nvim|nvim|code|host",
            "dev|@2|zsh|1|b25f,80x24,0,0,2|/h/app||zsh|host|host",
        ]);
        let expected = Layout::parse(indoc! {"
            session:
              name: dev
              cwd: ~/app
            windows:
              - name: editor
                layout: c195,80x24,0,0[80x12,0,0,0,80x11,0,13,1]
                panes:
                  - {}
                  - name: code
                    command: nvim
              - {}
        "})
        .unwrap();
        assert_eq!(layout, expected);
    }

    #[test]
    fn capture_lifts_shared_directories_to_the_window_and_session() {
        let layout = capture(&[
            "s|@1|a|0|l|/h/s||zsh|h|h",
            "s|@1|a|0|l|/h/s||zsh|h|h",
            "s|@2|b|0|l|/w||zsh|h|h",
            "s|@2|b|0|l|/w||zsh|h|h",
            "s|@2|b|0|l|/h/s||zsh|h|h",
            "s|@3|c|0|l|/h||zsh|h|h",
        ]);
        assert_eq!(layout.session.cwd.as_deref(), Some("~/s"));
        assert_eq!(layout.windows[0].cwd, None);
        assert_eq!(
            layout.windows[0].panes,
            vec![Pane::default(), Pane::default()]
        );
        assert_eq!(layout.windows[1].cwd.as_deref(), Some("/w"));
        assert_eq!(
            layout.windows[1].panes,
            vec![
                pane(None, None, None),
                pane(None, None, None),
                pane(None, None, Some("~/s"))
            ]
        );
        assert_eq!(layout.windows[2].cwd.as_deref(), Some("~"));
        assert_eq!(layout.windows[2].panes, vec![]);
    }

    #[test]
    fn capture_prefers_the_parent_directory_on_ties() {
        let layout = capture(&["s|@1|a|0|l|/a||zsh|h|h", "s|@1|a|0|l|/b||zsh|h|h"]);
        assert_eq!(layout.session.cwd.as_deref(), Some("/a"));
        assert_eq!(layout.windows[0].cwd, None);
        assert_eq!(
            layout.windows[0].panes,
            vec![pane(None, None, None), pane(None, None, Some("/b"))]
        );
    }

    #[test]
    fn capture_drops_shells() {
        for shell in ["zsh", "bash", "fish", "sh", "-zsh", ""] {
            let line = format!("s|@1|a|0|l|/a||{shell}|h|h");
            assert_eq!(capture(&[&line]).windows[0].panes, vec![], "{shell}");
        }
    }

    #[test]
    fn capture_prefers_the_start_command() {
        let layout = capture(&[
            r#"s|@1|a|0|l|/a|"tig --all \$X"|tig|h|h"#,
            "s|@1|a|0|l|/a|sleep 100|sleep|h|h",
            "s|@1|a|0|l|/a|'nix develop -c /bin/zsh -c tig'|nix|h|h",
            "s|@1|a|0|l|/a||htop|h|h",
        ]);
        let commands: Vec<_> = layout.windows[0]
            .panes
            .iter()
            .map(|p| p.command.as_deref())
            .collect();
        assert_eq!(
            commands,
            vec![
                Some("tig --all $X"),
                Some("sleep 100"),
                Some("tig"),
                Some("htop")
            ]
        );
    }

    #[test]
    fn capture_drops_default_titles_and_automatic_names() {
        let layout = capture(&[
            "s|@1|zsh|1|l|/a||zsh|my-host|my-host",
            "s|@2|named|0|l|/a||zsh|title|my-host",
        ]);
        assert_eq!(layout.windows[0], Window::default());
        assert_eq!(layout.windows[1].name.as_deref(), Some("named"));
        assert_eq!(
            layout.windows[1].panes,
            vec![pane(Some("title"), None, None)]
        );
    }

    #[test]
    fn capture_keeps_paths_outside_the_home_directory() {
        let layout = Layout::capture(&output(&["s|@1|a|0|l|/h/a||zsh|h|h"]), &Environment::new());
        assert_eq!(layout.unwrap().session.cwd.as_deref(), Some("/h/a"));
    }

    #[test]
    fn capture_fails_for_unexpected_output() {
        let err = Layout::capture("", &env()).unwrap_err();
        assert!(err.to_string().contains("no panes"));

        let err = Layout::capture("garbage", &env()).unwrap_err();
        assert!(err.to_string().contains("unexpected output from tmux"));
    }

    #[test]
    fn capture_writes_a_layout_switch_reads() {
        let layout = capture(&[
            "dev|@1|a: b|0|l|/h/x y||zsh|h|h",
            "dev|@1|a: b|0|l|/h/x y|\"echo '#1'\"|echo|~|h",
        ]);
        let text = serde_yaml_ng::to_string(&layout).unwrap();
        assert_eq!(Layout::parse(&text).unwrap(), layout);
    }
}
