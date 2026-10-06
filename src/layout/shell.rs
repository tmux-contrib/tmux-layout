use std::collections::HashMap;

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

/// Quotes `value` for POSIX shells when it holds anything but safe characters, so it survives
/// being parsed again unchanged.
pub fn quote(value: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !value.is_empty() && value.chars().all(safe) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Returns the shell command tmux runs in a pane for `command`. In a Nix dev shell, it runs
/// through `nix develop` and the user's shell, so the tools of the dev shell are available.
pub fn pane_command(command: &str, env: &Environment) -> String {
    if var(env, "IN_NIX_SHELL").is_none() {
        return command.to_string();
    }

    let shell = var(env, "SHELL").unwrap_or("bash");
    format!("nix develop -c {} -c {}", quote(shell), quote(command))
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
    fn pane_command_passes_the_command_through() {
        assert_eq!(pane_command("tig", &env(&[("IN_NIX_SHELL", "")])), "tig");
    }

    #[test]
    fn pane_command_runs_in_nix_develop_inside_a_nix_shell() {
        let env = env(&[("IN_NIX_SHELL", "impure"), ("SHELL", "/bin/zsh")]);
        assert_eq!(
            pane_command("tig --all | less", &env),
            "nix develop -c /bin/zsh -c 'tig --all | less'"
        );
    }

    #[test]
    fn pane_command_defaults_to_bash_inside_a_nix_shell() {
        let env = env(&[("IN_NIX_SHELL", "pure")]);
        assert_eq!(pane_command("tig", &env), "nix develop -c bash -c tig");
    }
}
