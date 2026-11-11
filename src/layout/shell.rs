use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Environment variables, by name.
pub type Environment = HashMap<String, String>;

/// Returns the value of `name` in `env`, unless it is unset or empty.
pub fn var<'a>(env: &'a Environment, name: &str) -> Option<&'a str> {
    env.get(name).map(String::as_str).filter(|v| !v.is_empty())
}

/// Replaces `$VAR` and `${VAR}` in `text` with the value of `VAR` in `env`, or nothing when it
/// is unset, like `envsubst`. Anything else, like `${VAR:-default}`, is left as is.
pub fn substitute(text: &str, env: &Environment) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('$') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let (name, len) = match after.strip_prefix('{') {
            Some(braced) => {
                let name = identifier(braced);
                match braced[name.len()..].starts_with('}') {
                    true if !name.is_empty() => (name, name.len() + 2),
                    _ => ("", 0),
                }
            }
            None => {
                let name = identifier(after);
                (name, name.len())
            }
        };
        if name.is_empty() {
            output.push('$');
        } else {
            output.push_str(env.get(name).map(String::as_str).unwrap_or_default());
        }
        rest = &after[len..];
    }
    output.push_str(rest);
    output
}

/// Returns the shell identifier at the start of `text`, if any.
fn identifier(text: &str) -> &str {
    let len = text
        .char_indices()
        .find(|&(i, c)| !(c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())))
        .map_or(text.len(), |(i, _)| i);
    &text[..len]
}

/// Replaces a leading `~` in `path` with the home directory in `env`. `~user` is not supported.
pub fn expand_home(path: &str, env: &Environment) -> String {
    let Some(home) = var(env, "HOME") else {
        return path.to_string();
    };
    match path.strip_prefix('~') {
        Some("") => home.to_string(),
        Some(rest) if rest.starts_with('/') => format!("{home}{rest}"),
        _ => path.to_string(),
    }
}

/// Returns `path` with the home directory in `env` written as `~`, so a layout works for the
/// same directory on other machines.
pub fn tilde(path: &Path, env: &Environment) -> String {
    let rest = var(env, "HOME").and_then(|home| path.strip_prefix(home).ok());
    match rest {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Quotes `value` for POSIX shells when it holds anything but safe characters, so it survives
/// being parsed again unchanged.
pub fn quote(value: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !value.is_empty() && value.chars().all(safe) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Splits `text` into words like a shell: on blanks outside quotes. Single quotes keep what
/// they hold as is; elsewhere, a backslash escapes the next character. This reads what
/// [`quote`] writes, and the commands tmux prints, like `"tig \$DIR"`.
pub fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut open: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (open, c) {
            (Some('\''), '\'') | (Some('"'), '"') => open = None,
            (Some('\''), c) => word.get_or_insert_default().push(c),
            (_, '\\') => word.get_or_insert_default().extend(chars.next()),
            (None, '\'' | '"') => {
                open = Some(c);
                word.get_or_insert_default();
            }
            (None, c) if c.is_whitespace() => words.extend(word.take()),
            (_, c) => word.get_or_insert_default().push(c),
        }
    }
    words.extend(word);
    words
}

/// OnExit is what happens to a pane when its command exits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OnExit {
    /// The pane closes, like tmux does.
    #[default]
    Close,
    /// The pane stays open, running the user's shell.
    Shell,
}

/// When is the condition a window must meet to be created.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    /// The window is always created.
    #[default]
    Always,
    /// The window is created unless the session already has a window with its name.
    NotExists,
}

/// CommandWhen is the condition a pane command must meet to run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandWhen {
    /// The command always runs.
    #[default]
    Always,
    /// The command runs if its program is found, and the pane opens the shell if not.
    Exists,
}

/// Returns whether `program` can run: a path to an executable file, or else the name of one in
/// a directory of the `PATH` in `env`.
pub fn program_exists(program: &str, env: &Environment) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let executable = |path: &Path| {
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return executable(Path::new(program));
    }
    var(env, "PATH")
        .is_some_and(|path| std::env::split_paths(path).any(|dir| executable(&dir.join(program))))
}

/// Launch is what a pane runs and what it does when that exits, before it is written as the
/// shell command tmux runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// The shell command.
    pub command: String,
    /// What the pane does when the command exits.
    pub on_exit: OnExit,
}

impl Launch {
    /// Returns the launch of `command`.
    pub fn new(command: &str, on_exit: OnExit) -> Self {
        Self {
            command: command.to_string(),
            on_exit,
        }
    }

    /// Returns the shell command tmux runs in a pane. In a Nix dev shell, it runs through
    /// `nix develop` and the user's shell, so the tools of the dev shell are available. With
    /// [`OnExit::Shell`], the user's shell replaces the command when it exits. The command is
    /// then quoted as one word and run with `-c`, so nothing in it, like a trailing comment,
    /// can swallow the `exec`.
    pub fn render(&self, env: &Environment) -> String {
        let shell = var(env, "SHELL");
        let command = &self.command;
        let run = match (var(env, "IN_NIX_SHELL"), self.on_exit) {
            (Some(_), _) => format!(
                "nix develop -c {} -c {}",
                quote(shell.unwrap_or("bash")),
                quote(command)
            ),
            (None, OnExit::Close) => command.to_string(),
            (None, OnExit::Shell) => {
                format!("{} -c {}", quote(shell.unwrap_or("sh")), quote(command))
            }
        };
        match self.on_exit {
            OnExit::Close => run,
            OnExit::Shell => format!("{run}; exec {}", quote(shell.unwrap_or("sh"))),
        }
    }

    /// Returns the launch [`Launch::render`] wrote as `rendered`, or `rendered` itself as the
    /// command when it did not. Only the exact forms it writes are unwrapped, so a command
    /// that merely contains `; exec` or `sh -c` is left as is.
    pub fn parse(rendered: &str) -> Self {
        if let Some((head, shell)) = rendered.rsplit_once("; exec ") {
            if let [shell] = &words(shell)[..] {
                let head = words(head);
                match head.iter().map(String::as_str).collect::<Vec<_>>()[..] {
                    ["nix", "develop", "-c", _, "-c", inner] => {
                        return Self::new(inner, OnExit::Shell)
                    }
                    [run, "-c", inner] if run == shell => return Self::new(inner, OnExit::Shell),
                    _ => {}
                }
            }
        }
        let words = words(rendered);
        match words.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["nix", "develop", "-c", _, "-c", inner] => Self::new(inner, OnExit::Close),
            _ => Self::new(rendered, OnExit::Close),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> Environment {
        vars.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn substitute_replaces_variables() {
        let env = env(&[("USER", "me"), ("DIR", "/code")]);
        assert_eq!(substitute("${USER}-dev", &env), "me-dev");
        assert_eq!(substitute("cd $DIR/app", &env), "cd /code/app");
        assert_eq!(substitute("$USER$USER", &env), "meme");
    }

    #[test]
    fn substitute_replaces_unset_variables_with_nothing() {
        assert_eq!(substitute("a${MISSING}b$MISSING", &env(&[])), "ab");
    }

    #[test]
    fn substitute_leaves_other_forms_alone() {
        let env = env(&[("EDITOR", "nvim")]);
        assert_eq!(substitute("${EDITOR:-vim}", &env), "${EDITOR:-vim}");
        assert_eq!(substitute("${EDITOR", &env), "${EDITOR");
        assert_eq!(substitute("${}", &env), "${}");
        assert_eq!(substitute("cost: $5 $", &env), "cost: $5 $");
        assert_eq!(substitute("ünïcode $EDITOR", &env), "ünïcode nvim");
    }

    #[test]
    fn expand_home_replaces_a_leading_tilde() {
        let env = env(&[("HOME", "/h")]);
        assert_eq!(expand_home("~", &env), "/h");
        assert_eq!(expand_home("~/foo/bar", &env), "/h/foo/bar");
    }

    #[test]
    fn expand_home_leaves_other_paths_alone() {
        let env = env(&[("HOME", "/h")]);
        assert_eq!(expand_home("/etc/foo", &env), "/etc/foo");
        assert_eq!(expand_home("./services/api", &env), "./services/api");
        assert_eq!(expand_home("~root/foo", &env), "~root/foo");
        assert_eq!(expand_home("~/foo", &Environment::new()), "~/foo");
    }

    #[test]
    fn quote_leaves_safe_values_alone() {
        assert_eq!(quote("/bin/sh"), "/bin/sh");
        assert_eq!(quote("tig"), "tig");
    }

    #[test]
    fn quote_quotes_shell_metacharacters() {
        assert_eq!(quote("tig --all | less"), "'tig --all | less'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn render_passes_the_command_through() {
        assert_eq!(
            Launch::new("tig", OnExit::Close).render(&env(&[("IN_NIX_SHELL", "")])),
            "tig"
        );
    }

    #[test]
    fn render_runs_in_nix_develop_inside_a_nix_shell() {
        let env = env(&[("IN_NIX_SHELL", "impure"), ("SHELL", "/bin/zsh")]);
        assert_eq!(
            Launch::new("tig --all | less", OnExit::Close).render(&env),
            "nix develop -c /bin/zsh -c 'tig --all | less'"
        );
    }

    #[test]
    fn render_defaults_to_bash_inside_a_nix_shell() {
        let env = env(&[("IN_NIX_SHELL", "pure")]);
        assert_eq!(
            Launch::new("tig", OnExit::Close).render(&env),
            "nix develop -c bash -c tig"
        );
    }

    #[test]
    fn tilde_writes_the_home_directory_as_a_tilde() {
        let env = env(&[("HOME", "/h")]);
        assert_eq!(tilde(Path::new("/h"), &env), "~");
        assert_eq!(tilde(Path::new("/h/code/app"), &env), "~/code/app");
        assert_eq!(tilde(Path::new("/hx/code"), &env), "/hx/code");
        assert_eq!(tilde(Path::new("/h/code"), &Environment::new()), "/h/code");
    }

    #[test]
    fn words_splits_on_blanks_outside_quotes() {
        assert_eq!(words("  tig  --all "), vec!["tig", "--all"]);
        assert_eq!(
            words(r#"a 'b c' "d e" f\ g ''"#),
            vec!["a", "b c", "d e", "f g", ""]
        );
        assert!(words("").is_empty());
    }

    #[test]
    fn words_reads_the_escaping_of_tmux() {
        assert_eq!(
            words(r#""sleep 1 # \$HOME \"q\" 'it' \\ ~""#),
            vec![r#"sleep 1 # $HOME "q" 'it' \ ~"#]
        );
        assert_eq!(words(r"\~/bin/x"), vec!["~/bin/x"]);
    }

    #[test]
    fn words_reads_what_quote_writes() {
        for value in ["tig", "tig --all | less", "it's", "", r"a\b"] {
            assert_eq!(words(&quote(value)), vec![value]);
        }
    }

    #[test]
    fn program_exists_looks_in_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        let env = env(&[("PATH", &format!("/nonexistent:{}", dir.path().display()))]);
        assert!(!program_exists("tool", &env));
        std::fs::set_permissions(&tool, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        assert!(program_exists("tool", &env));
        assert!(program_exists(
            tool.to_str().unwrap(),
            &super::Environment::new()
        ));
        assert!(!program_exists("other", &env));
        assert!(!program_exists("tool", &super::Environment::new()));
    }

    #[test]
    fn render_falls_back_to_the_shell_on_exit() {
        let env = env(&[("SHELL", "/bin/zsh")]);
        assert_eq!(
            Launch::new("tig", OnExit::Shell).render(&env),
            "/bin/zsh -c tig; exec /bin/zsh"
        );
        assert_eq!(
            Launch::new("tig", OnExit::Shell).render(&super::Environment::new()),
            "sh -c tig; exec sh"
        );
    }

    #[test]
    fn render_reaches_the_shell_after_a_trailing_comment() {
        use std::os::unix::fs::PermissionsExt;

        // A shell that runs commands, and says so when it is started without one
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("shell");
        std::fs::write(
            &shell,
            "#!/bin/sh\nif [ \"$1\" = -c ]; then sh -c \"$2\"; else echo REACHED; fi\n",
        )
        .unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let env = env(&[("SHELL", shell.to_str().unwrap())]);

        for command in ["echo hi # it's a comment", "echo hi &", "echo 'hi'"] {
            let run = Launch::new(command, OnExit::Shell).render(&env);
            let output = std::process::Command::new("sh")
                .args(["-c", &run])
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("hi"), "{command}: {stdout}");
            assert!(stdout.contains("REACHED"), "{command}: {stdout}");
        }
    }

    #[test]
    fn parse_undoes_render() {
        let nix = env(&[("IN_NIX_SHELL", "impure"), ("SHELL", "/bin/zsh")]);
        let plain = env(&[("SHELL", "/bin/zsh")]);
        for env in [&nix, &plain] {
            for on_exit in [OnExit::Close, OnExit::Shell] {
                for command in ["tig", "tig --all | less", "echo 'hi' # it's"] {
                    assert_eq!(
                        Launch::parse(&Launch::new(command, on_exit).render(env)),
                        Launch::new(command, on_exit)
                    );
                }
            }
        }
    }

    #[test]
    fn parse_leaves_other_commands_as_they_are() {
        for command in [
            "tig --all",
            "nix develop",
            "foo; exec zsh",
            "sh -c foo",
            "zsh -c foo; exec bash",
        ] {
            assert_eq!(Launch::parse(command), Launch::new(command, OnExit::Close));
        }
    }
}
