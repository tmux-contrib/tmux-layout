use crate::app::args::*;
use crate::layout::*;
use crate::log::{count, debug, info, success};
use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The layout `new` writes; `{{name}}` and `{{cwd}}` are replaced with YAML values.
const STARTER_LAYOUT: &str = include_str!("layout.yml");

/// Format of the ids tmux prints for a new window: its session, itself and its first pane.
const WINDOW_IDS: &str = "#{session_id} #{window_id} #{pane_id}";

/// Returns `args` as owned strings, for [`Tmux`].
fn strings<const N: usize>(args: [&str; N]) -> Vec<String> {
    args.map(str::to_string).to_vec()
}

/// Apply a layout.
pub struct SwitchCommand {
    /// Tmux the layout is applied to.
    pub tmux: Box<dyn Tmux>,
    /// Environment the layout file and pane commands are resolved against.
    pub environment: Environment,
}

impl SwitchCommand {
    /// Execute the SwitchCommand with the provided arguments.
    pub fn execute(&mut self, args: &SwitchCommandArgs) -> Result<()> {
        let path = find(&args.parent.layout_dir, &args.name)?;
        debug(format!("layout: {}", path.display()));
        let layout = Layout::read_from_file(&path, &self.environment)?;
        let windows = layout.plan(&self.environment);

        // Inside tmux, add the windows to the current session
        if var(&self.environment, "TMUX").is_some() {
            let session = self
                .tmux
                .run(&strings(["display-message", "-p", "#{session_id}"]))?;
            for window in &windows {
                self.new_window(&session, window)?;
            }
            return Ok(());
        }

        // Outside tmux, attach to the session, after creating it unless it exists. An existing
        // session is left as is, so switching again is safe.
        let name = layout.session_name();
        let target = format!("={name}");
        if self
            .tmux
            .run(&strings(["has-session", "-t", &target]))
            .is_ok()
        {
            info(format!("session '{name}' already exists; attaching"));
            return self.tmux.exec(&strings(["attach-session", "-t", &target]));
        }

        let Some((first, rest)) = windows.split_first() else {
            bail!("{} must declare at least one window", path.display());
        };
        let session = self.create(strings(["new-session", "-d", "-s", name]), first)?;
        for window in rest {
            self.new_window(&session, window)?;
        }
        self.tmux.exec(&strings(["attach-session", "-t", &session]))
    }

    /// Adds `window` to the session with id `session`.
    fn new_window(&self, session: &str, window: &WindowPlan) -> Result<()> {
        let target = format!("{session}:");
        self.create(strings(["new-window", "-d", "-t", &target]), window)?;
        Ok(())
    }

    /// Creates `window` with `command` (`new-session` or `new-window`), then its other panes.
    /// Returns the id of the session the window belongs to.
    fn create(&self, mut command: Vec<String>, window: &WindowPlan) -> Result<String> {
        let (first, rest) = window
            .panes
            .split_first()
            .context("a window must have a pane")?;

        command.extend(strings(["-P", "-F", WINDOW_IDS]));
        if let Some(name) = &window.name {
            command.extend(strings(["-n", name]));
        }
        let output = self.tmux.run(&pane_args(command, first))?;
        let ids: Vec<&str> = output.split_whitespace().collect();
        let [session, window_id, pane_id] = ids[..] else {
            bail!("unexpected output from tmux: {output}");
        };
        self.set_title(pane_id, first)?;

        // Without -d, each new pane becomes active and is the one split next
        for pane in rest {
            let command = strings(["split-window", "-t", window_id, "-P", "-F", "#{pane_id}"]);
            let pane_id = self.tmux.run(&pane_args(command, pane))?;
            self.set_title(&pane_id, pane)?;
        }

        if let Some(layout) = &window.layout {
            self.tmux
                .run(&strings(["select-layout", "-t", window_id, layout]))?;
        }
        Ok(session.to_string())
    }

    /// Sets the title of the pane with id `pane_id`, if `pane` has one.
    fn set_title(&self, pane_id: &str, pane: &PanePlan) -> Result<()> {
        if let Some(title) = &pane.title {
            self.tmux
                .run(&strings(["select-pane", "-t", pane_id, "-T", title]))?;
        }
        Ok(())
    }
}

/// Appends the working directory and the command of `pane` to `command`, which creates it.
fn pane_args(mut command: Vec<String>, pane: &PanePlan) -> Vec<String> {
    if let Some(cwd) = &pane.cwd {
        command.extend(strings(["-c", cwd]));
    }
    command.extend(pane.command.clone());
    command
}

/// List the available layouts.
pub struct ListCommand {
    /// Writer used to output the layout names.
    pub writer: Box<dyn Write>,
}

impl ListCommand {
    /// Execute the ListCommand with the provided arguments.
    pub fn execute(&mut self, args: &ListCommandArgs) -> Result<()> {
        for name in list(&args.parent.layout_dir)? {
            writeln!(self.writer, "{name}")?;
        }
        Ok(())
    }
}

/// Returns `value` as a YAML scalar, quoted if it needs to be.
fn yaml_scalar(value: &str) -> Result<String> {
    Ok(serde_yaml_ng::to_string(value)?.trim_end().to_string())
}

/// Returns `path` with the home directory in `env` written as `~`, so the layout works for
/// the same directory on other machines.
fn tilde(path: &Path, env: &Environment) -> String {
    let rest = var(env, "HOME").and_then(|home| path.strip_prefix(home).ok());
    match rest {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Create a starter layout.
pub struct NewCommand {
    /// Directory the session of the layout opens in.
    pub cwd: PathBuf,
    /// Environment holding the home directory.
    pub environment: Environment,
}

impl NewCommand {
    /// Execute the NewCommand with the provided arguments.
    pub fn execute(&mut self, args: &NewCommandArgs) -> Result<()> {
        let (dir, name) = (&args.parent.layout_dir, &args.name);
        check_name(name)?;
        let path = match find(dir, name) {
            Ok(path) if !args.force => bail!(
                "{} already exists (edit it with `tmux-layout edit {name}`, or replace it with --force)",
                path.display()
            ),
            Ok(path) => path,
            Err(_) => dir.join(format!("{name}.yml")),
        };

        let layout = STARTER_LAYOUT
            .replace("{{name}}", &yaml_scalar(name)?)
            .replace(
                "{{cwd}}",
                &yaml_scalar(&tilde(&self.cwd, &self.environment))?,
            );
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        std::fs::write(&path, layout)
            .with_context(|| format!("failed to write {}", path.display()))?;

        success(format!("Created {}", path.display()));
        info(format!(
            "declare your windows with `tmux-layout edit {name}`, then open them with `tmux-layout switch {name}`"
        ));
        Ok(())
    }
}

/// Open a layout in an editor, then check it.
pub struct EditCommand {
    /// Editor command, which may include arguments (e.g. `code --wait`).
    pub editor: String,
    /// Environment the layout is checked against, as `switch` reads it.
    pub environment: Environment,
}

impl EditCommand {
    /// Execute the EditCommand with the provided arguments.
    pub fn execute(&mut self, args: &EditCommandArgs) -> Result<()> {
        let name = &args.name;
        let path = match find(&args.parent.layout_dir, name) {
            Ok(path) => path,
            Err(err) => bail!("{err} (create it with `tmux-layout new {name}`)"),
        };

        // Run the editor through the shell, so commands with arguments work
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{} \"$1\"", self.editor))
            .arg("sh")
            .arg(&path)
            .status()
            .with_context(|| format!("failed to run the editor ({})", self.editor))?;
        if !status.success() {
            bail!("the editor ({}) exited with {status}", self.editor);
        }

        let layout = match Layout::read_from_file(&path, &self.environment) {
            Ok(layout) => layout,
            Err(err) => bail!("{err:#} (fix it with `tmux-layout edit {name}`)"),
        };
        success(format!(
            "{} is valid ({})",
            path.display(),
            count(layout.windows.len(), "window")
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use indoc::indoc;
    use std::cell::{Cell, RefCell};
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Writer(Arc<Mutex<Vec<u8>>>);

    impl Writer {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }

        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// FakeTmux records the commands it is given and answers them like a tmux server holding
    /// session `$1`, plus the session `existing` if set.
    #[derive(Clone, Default)]
    struct FakeTmux {
        calls: Rc<RefCell<Vec<String>>>,
        existing: Option<&'static str>,
        windows: Rc<Cell<usize>>,
        panes: Rc<Cell<usize>>,
    }

    impl FakeTmux {
        fn existing(name: &'static str) -> Self {
            Self {
                existing: Some(name),
                ..Default::default()
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn next(counter: &Cell<usize>) -> usize {
            counter.set(counter.get() + 1);
            counter.get()
        }
    }

    impl Tmux for FakeTmux {
        fn run(&self, args: &[String]) -> Result<String> {
            self.calls.borrow_mut().push(args.join(" "));
            match args[0].as_str() {
                "display-message" => Ok("$1".to_string()),
                "has-session" => match self.existing {
                    Some(name) if args[2] == format!("={name}") => Ok(String::new()),
                    _ => bail!("can't find session"),
                },
                "new-session" | "new-window" => Ok(format!(
                    "$1 @{} %{}",
                    Self::next(&self.windows),
                    Self::next(&self.panes)
                )),
                "split-window" => Ok(format!("%{}", Self::next(&self.panes))),
                _ => Ok(String::new()),
            }
        }

        fn exec(&self, args: &[String]) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("exec {}", args.join(" ")));
            Ok(())
        }
    }

    const LAYOUT: &str = indoc! {"
        session:
          name: demo
          cwd: ~/code
        windows:
          - name: editor
            layout: tiled
            panes:
              - name: git
                command: tig
              - command: nvim
                cwd: /src
          - name: notes
    "};

    fn layout_dir(layout: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("demo.yml"), layout).unwrap();
        dir
    }

    fn switch_args(dir: &Path, name: &str) -> SwitchCommandArgs {
        let dir = dir.to_str().unwrap();
        let program = Program::parse_from(["tmux-layout", "switch", "-d", dir, name]);
        let ProgramCommand::Switch(args) = program.command else {
            unreachable!()
        };
        args
    }

    fn switch(tmux: &FakeTmux, env: &[(&str, &str)], name: &str) -> Result<()> {
        let dir = layout_dir(LAYOUT);
        let mut command = SwitchCommand {
            tmux: Box::new(tmux.clone()),
            environment: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        };
        command.execute(&switch_args(dir.path(), name))
    }

    #[test]
    fn switch_creates_and_attaches_a_session_outside_tmux() {
        let tmux = FakeTmux::default();
        switch(&tmux, &[("HOME", "/h")], "demo").unwrap();
        assert_eq!(
            tmux.calls(),
            vec![
                "has-session -t =demo",
                "new-session -d -s demo -P -F #{session_id} #{window_id} #{pane_id} -n editor -c /h/code tig",
                "select-pane -t %1 -T git",
                "split-window -t @1 -P -F #{pane_id} -c /src nvim",
                "select-layout -t @1 tiled",
                "new-window -d -t $1: -P -F #{session_id} #{window_id} #{pane_id} -n notes -c /h/code",
                "exec attach-session -t $1",
            ]
        );
    }

    #[test]
    fn switch_attaches_to_an_existing_session() {
        let tmux = FakeTmux::existing("demo");
        switch(&tmux, &[], "demo").unwrap();
        assert_eq!(
            tmux.calls(),
            vec!["has-session -t =demo", "exec attach-session -t =demo"]
        );
    }

    #[test]
    fn switch_adds_the_windows_to_the_current_session_inside_tmux() {
        let tmux = FakeTmux::existing("demo");
        switch(&tmux, &[("TMUX", "/tmp/tmux-501/default,1,0")], "demo").unwrap();
        assert_eq!(
            tmux.calls(),
            vec![
                "display-message -p #{session_id}",
                "new-window -d -t $1: -P -F #{session_id} #{window_id} #{pane_id} -n editor -c ~/code tig",
                "select-pane -t %1 -T git",
                "split-window -t @1 -P -F #{pane_id} -c /src nvim",
                "select-layout -t @1 tiled",
                "new-window -d -t $1: -P -F #{session_id} #{window_id} #{pane_id} -n notes -c ~/code",
            ]
        );
    }

    #[test]
    fn switch_runs_pane_commands_in_nix_develop_inside_a_nix_shell() {
        let tmux = FakeTmux::default();
        let env = [("IN_NIX_SHELL", "impure"), ("SHELL", "/bin/zsh")];
        switch(&tmux, &env, "demo").unwrap();
        assert!(tmux.calls()[1].ends_with(" nix develop -c /bin/zsh -c tig"));
    }

    #[test]
    fn switch_fails_for_a_missing_layout() {
        let tmux = FakeTmux::default();
        let err = switch(&tmux, &[], "nope").unwrap_err();
        assert!(err.to_string().contains("layout 'nope' not found"));
        assert!(tmux.calls().is_empty());
    }

    #[test]
    fn list_prints_the_layout_names() {
        let dir = layout_dir(LAYOUT);
        std::fs::write(dir.path().join("work.yaml"), "").unwrap();
        let writer = Writer::new();
        let program =
            Program::parse_from(["tmux-layout", "list", "-d", dir.path().to_str().unwrap()]);
        let ProgramCommand::List(args) = program.command else {
            unreachable!()
        };
        let mut command = ListCommand {
            writer: Box::new(writer.clone()),
        };
        command.execute(&args).unwrap();
        assert_eq!(writer.contents(), "demo\nwork\n");
    }

    fn env(vars: &[(&str, &str)]) -> Environment {
        vars.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn new(dir: &Path, name: &str, force: bool, cwd: &str) -> Result<()> {
        let dir = dir.to_str().unwrap();
        let mut argv = vec!["tmux-layout", "new", "-d", dir, name];
        if force {
            argv.push("--force");
        }
        let ProgramCommand::New(args) = Program::parse_from(argv).command else {
            unreachable!()
        };
        let mut command = NewCommand {
            cwd: PathBuf::from(cwd),
            environment: env(&[("HOME", "/h")]),
        };
        command.execute(&args)
    }

    fn edit(dir: &Path, name: &str, editor: &str) -> Result<()> {
        let dir = dir.to_str().unwrap();
        let program = Program::parse_from(["tmux-layout", "edit", "-d", dir, name]);
        let ProgramCommand::Edit(args) = program.command else {
            unreachable!()
        };
        let mut command = EditCommand {
            editor: editor.to_string(),
            environment: env(&[]),
        };
        command.execute(&args)
    }

    #[test]
    fn new_creates_a_starter_layout() {
        let dir = tempfile::tempdir().unwrap();
        let layouts = dir.path().join("layouts");
        new(&layouts, "dev", false, "/h/code").unwrap();

        let layout = Layout::read_from_file(&layouts.join("dev.yml"), &env(&[])).unwrap();
        assert_eq!(layout.session_name(), "dev");
        assert_eq!(layout.session.cwd.as_deref(), Some("~/code"));
        assert_eq!(layout.windows.len(), 2);
    }

    #[test]
    fn new_quotes_values_yaml_would_misread() {
        let dir = tempfile::tempdir().unwrap();
        new(dir.path(), "123", false, "/srv/a: b #c").unwrap();

        let layout = Layout::read_from_file(&dir.path().join("123.yml"), &env(&[])).unwrap();
        assert_eq!(layout.session_name(), "123");
        assert_eq!(layout.session.cwd.as_deref(), Some("/srv/a: b #c"));
    }

    #[test]
    fn new_writes_the_home_directory_as_a_tilde() {
        let dir = tempfile::tempdir().unwrap();
        new(dir.path(), "home", false, "/h").unwrap();
        let layout = Layout::read_from_file(&dir.path().join("home.yml"), &env(&[])).unwrap();
        assert_eq!(layout.session.cwd.as_deref(), Some("~"));
    }

    #[test]
    fn new_replaces_a_layout_only_with_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dev.yaml");
        std::fs::write(&path, "mine").unwrap();

        let err = new(dir.path(), "dev", false, "/h").unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");

        new(dir.path(), "dev", true, "/h").unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("session:"));
        assert!(!dir.path().join("dev.yml").exists());
    }

    #[test]
    fn new_rejects_names_list_would_not_show() {
        let dir = tempfile::tempdir().unwrap();
        let err = new(dir.path(), "../dev", false, "/h").unwrap_err();
        assert!(err.to_string().contains("invalid layout name"));
    }

    #[test]
    fn edit_checks_the_layout_after_the_editor_exits() {
        let dir = layout_dir(LAYOUT);
        edit(dir.path(), "demo", "true").unwrap();
    }

    #[test]
    fn edit_runs_editors_with_arguments() {
        let dir = layout_dir(LAYOUT);
        let invalid = dir.path().join("invalid");
        std::fs::write(&invalid, "windows: [{}]").unwrap();

        // The editor "saves" the invalid layout over the valid one
        let editor = format!("cp '{}'", invalid.display());
        let err = edit(dir.path(), "demo", &editor).unwrap_err();
        assert!(err.to_string().contains("session.name is required"));
        assert!(err
            .to_string()
            .contains("fix it with `tmux-layout edit demo`"));
    }

    #[test]
    fn edit_fails_when_the_editor_fails() {
        let dir = layout_dir(LAYOUT);
        let err = edit(dir.path(), "demo", "false").unwrap_err();
        assert!(err.to_string().contains("the editor (false) exited"));
    }

    #[test]
    fn edit_fails_for_a_missing_layout() {
        let dir = layout_dir(LAYOUT);
        let err = edit(dir.path(), "nope", "true").unwrap_err();
        assert!(err.to_string().contains("layout 'nope' not found"));
        assert!(err
            .to_string()
            .contains("create it with `tmux-layout new nope`"));
    }
}
