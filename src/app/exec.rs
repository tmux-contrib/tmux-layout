use crate::app::args::*;
use crate::layout::*;
use crate::log::{count, debug, error, info, success};
use anyhow::{bail, Context, Result};
use console::{measure_text_width, pad_str, style, truncate_str, Alignment};
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

/// Returns `value` as a YAML scalar, quoted if it needs to be.
fn yaml_scalar(value: &str) -> Result<String> {
    Ok(serde_yaml_ng::to_string(value)?.trim_end().to_string())
}

/// Returns the file to write the layout `name` in `dir` to. An existing layout is only
/// replaced with `force`.
fn new_path(dir: &Path, name: &str, force: bool) -> Result<PathBuf> {
    check_name(name)?;
    match find(dir, name) {
        Ok(path) if !force => bail!(
            "{} already exists (edit it with `tmux-layout edit {name}`, or replace it with --force)",
            path.display()
        ),
        Ok(path) => Ok(path),
        Err(_) => Ok(dir.join(format!("{name}.yml"))),
    }
}

/// Writes `layout` to `path` in `dir`, creating `dir` if needed.
fn write(dir: &Path, path: &Path, layout: &str) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    std::fs::write(path, layout).with_context(|| format!("failed to write {}", path.display()))
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
        let path = new_path(dir, name, args.force)?;

        let layout = STARTER_LAYOUT
            .replace("{{name}}", &yaml_scalar(name)?)
            .replace(
                "{{cwd}}",
                &yaml_scalar(&tilde(&self.cwd, &self.environment))?,
            );
        write(dir, &path, &layout)?;

        success(format!("Created {}", path.display()));
        info(format!(
            "declare your windows with `tmux-layout edit {name}`, then open them with `tmux-layout switch {name}`"
        ));
        Ok(())
    }
}

/// Confirm asks a yes/no question and returns the answer.
pub type Confirm = Box<dyn FnMut(&str) -> Result<bool>>;

/// Open a layout in an editor, then check it.
pub struct EditCommand {
    /// Editor command, which may include arguments (e.g. `code --wait`).
    pub editor: String,
    /// Environment the layout is checked against, as `switch` reads it.
    pub environment: Environment,
    /// Asks whether to re-open an invalid layout; unset when nobody can answer.
    pub confirm: Option<Confirm>,
}

impl EditCommand {
    /// Execute the EditCommand with the provided arguments.
    pub fn execute(&mut self, args: &EditCommandArgs) -> Result<()> {
        let name = &args.name;
        let path = match find(&args.parent.layout_dir, name) {
            Ok(path) => path,
            Err(err) => bail!("{err} (create it with `tmux-layout new {name}`)"),
        };

        loop {
            self.open(&path)?;
            let err = match Layout::read_from_file(&path, &self.environment) {
                Ok(layout) => {
                    success(format!(
                        "{} is valid ({})",
                        path.display(),
                        count(layout.windows.len(), "window")
                    ));
                    return Ok(());
                }
                Err(err) => err,
            };
            let Some(confirm) = &mut self.confirm else {
                bail!("{err:#} (fix it with `tmux-layout edit {name}`)");
            };
            error(format!("{err:#}"));
            if !confirm("Re-open the editor?")? {
                bail!(
                    "{} is invalid (fix it with `tmux-layout edit {name}`)",
                    path.display()
                );
            }
        }
    }

    /// Runs the editor on `path` and waits for it to exit.
    fn open(&self, path: &Path) -> Result<()> {
        // Run the editor through the shell, so commands with arguments work
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("{} \"$1\"", self.editor))
            .arg("sh")
            .arg(path)
            .status()
            .with_context(|| format!("failed to run the editor ({})", self.editor))?;
        if !status.success() {
            bail!("the editor ({}) exited with {status}", self.editor);
        }
        Ok(())
    }
}

/// Save a tmux session as a layout.
pub struct SaveCommand {
    /// Tmux the session is read from.
    pub tmux: Box<dyn Tmux>,
    /// Environment telling whether this runs inside tmux, and holding the home directory.
    pub environment: Environment,
}

impl SaveCommand {
    /// Execute the SaveCommand with the provided arguments.
    pub fn execute(&mut self, args: &SaveCommandArgs) -> Result<()> {
        let (dir, name) = (&args.parent.layout_dir, &args.name);
        let path = new_path(dir, name, args.force)?;

        // Without a target, tmux lists the session this runs in
        let mut command = strings(["list-panes", "-s", "-F", PANE_FORMAT]);
        match &args.target {
            Some(target) => command.extend(strings(["-t", target])),
            None if var(&self.environment, "TMUX").is_none() => bail!(
                "not inside tmux; run `tmux-layout save {name}` in a tmux pane, or name a session with --target"
            ),
            None => {}
        }
        let layout = Layout::capture(&self.tmux.run(&command)?, &self.environment)?;
        let text = format!(
            "# Open this layout with `tmux-layout switch {name}`.\n\n{}",
            serde_yaml_ng::to_string(&layout)?
        );
        write(dir, &path, &text)?;

        success(format!(
            "Saved {} ({})",
            path.display(),
            count(layout.windows.len(), "window")
        ));
        info(format!(
            "adjust it with `tmux-layout edit {name}`, then open it with `tmux-layout switch {name}`"
        ));
        Ok(())
    }
}

/// List the available layouts.
pub struct ListCommand {
    /// Writer used to output the layouts.
    pub writer: Box<dyn Write>,
    /// Whether to write a table for people instead of one name per line, for scripts.
    pub table: bool,
    /// Width of the terminal the table is cut to, if known.
    pub width: Option<usize>,
    /// Tmux asked which sessions are running, for the table.
    pub tmux: Box<dyn Tmux>,
    /// Environment the layout files are resolved against, as `switch` reads them.
    pub environment: Environment,
}

impl ListCommand {
    /// Execute the ListCommand with the provided arguments.
    pub fn execute(&mut self, args: &ListCommandArgs) -> Result<()> {
        let dir = &args.parent.layout_dir;
        let names = list(dir)?;
        if !self.table {
            for name in names {
                writeln!(self.writer, "{name}")?;
            }
            return Ok(());
        }

        // Without a tmux server, no session is running
        let sessions = self
            .tmux
            .run(&strings(["list-sessions", "-F", "#{session_name}"]))
            .unwrap_or_default();
        let sessions: Vec<&str> = sessions.lines().collect();

        let mut rows = vec![Row::header()];
        for name in &names {
            let layout =
                find(dir, name).and_then(|path| Layout::read_from_file(&path, &self.environment));
            rows.push(Row::new(name, layout, &self.environment, &sessions));
        }
        let widths: Vec<usize> = (0..Row::COLUMNS)
            .map(|i| {
                rows.iter()
                    .map(|row| measure_text_width(&row.cells[i]))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for row in &rows {
            writeln!(self.writer, "{}", row.render(&widths, self.width))?;
        }

        let running = rows.iter().filter(|row| row.running).count();
        let mut summary = count(names.len(), "layout");
        if running > 0 {
            summary.push_str(&format!(", {running} running"));
        }
        writeln!(self.writer, "\n{}", style(summary).dim())?;
        Ok(())
    }
}

/// Row is a line of the table `list` writes: the header, or a layout.
struct Row {
    /// Layout, session, windows, panes and working directory.
    cells: [String; Row::COLUMNS],
    /// Whether this is the header.
    header: bool,
    /// Whether the session of the layout is running.
    running: bool,
    /// Whether the layout is invalid; its error is in the last cell.
    invalid: bool,
}

impl Row {
    /// Number of cells in a row.
    const COLUMNS: usize = 5;

    /// Returns the header row.
    fn header() -> Self {
        Self {
            cells: ["LAYOUT", "SESSION", "WINDOWS", "PANES", "CWD"].map(str::to_string),
            header: true,
            running: false,
            invalid: false,
        }
    }

    /// Returns the row of the layout `name`, given whether it could be read and the running
    /// `sessions`.
    fn new(name: &str, layout: Result<Layout>, env: &Environment, sessions: &[&str]) -> Self {
        let none = || "—".to_string();
        match layout {
            Ok(layout) => {
                let panes: usize = layout.plan(env).iter().map(|w| w.panes.len()).sum();
                Self {
                    cells: [
                        name.to_string(),
                        layout.session_name().to_string(),
                        layout.windows.len().to_string(),
                        panes.to_string(),
                        layout.session_cwd().map_or_else(none, str::to_string),
                    ],
                    header: false,
                    running: sessions.contains(&layout.session_name()),
                    invalid: false,
                }
            }
            // The cause alone; the path is the layout directory and the name
            Err(err) => Self {
                cells: [
                    name.to_string(),
                    none(),
                    none(),
                    none(),
                    format!("✗ {}", err.root_cause()),
                ],
                header: false,
                running: false,
                invalid: true,
            },
        }
    }

    /// Returns the row as a line, its cells padded to `widths` and the last one cut so the line
    /// fits in `width`.
    fn render(&self, widths: &[usize], width: Option<usize>) -> String {
        let marker = match self.running {
            true => style("●").green().to_string(),
            false => " ".to_string(),
        };
        // The marker and the other cells, each followed by two spaces
        let used = 2 + widths[..Self::COLUMNS - 1]
            .iter()
            .map(|w| w + 2)
            .sum::<usize>();
        let rest = width.map_or(usize::MAX, |width| width.saturating_sub(used));
        let cells = self
            .cells
            .iter()
            .zip(widths)
            .enumerate()
            .map(|(i, (cell, &width))| {
                let last = i == Self::COLUMNS - 1;
                // Counts are aligned right, under their header
                let align = match i {
                    2 | 3 => Alignment::Right,
                    _ => Alignment::Left,
                };
                let text = match last {
                    true => truncate_str(cell, rest, "…").into_owned(),
                    false => pad_str(cell, width, align, None).into_owned(),
                };
                let text = style(text);
                match (i, self.header, self.invalid) {
                    (_, true, _) => text.dim(),
                    (0, _, _) => text.bold(),
                    (_, _, true) if last => text.red(),
                    (_, _, true) => text.dim(),
                    _ => text,
                }
                .to_string()
            });
        format!("{marker} {}", cells.collect::<Vec<_>>().join("  "))
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
    /// session `$1`, plus the session `existing` if set. Without a server, listing sessions
    /// fails.
    #[derive(Clone)]
    struct FakeTmux {
        calls: Rc<RefCell<Vec<String>>>,
        server: bool,
        existing: Option<&'static str>,
        windows: Rc<Cell<usize>>,
        panes: Rc<Cell<usize>>,
    }

    impl Default for FakeTmux {
        fn default() -> Self {
            Self {
                calls: Default::default(),
                server: true,
                existing: None,
                windows: Default::default(),
                panes: Default::default(),
            }
        }
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
                "list-panes" => Ok(LIST_PANES.replace('|', "\x1f")),
                "list-sessions" if !self.server => bail!("no server running"),
                "list-sessions" => Ok(["$1"]
                    .into_iter()
                    .chain(self.existing)
                    .collect::<Vec<_>>()
                    .join("\n")),
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

    /// What tmux lists for a session like [`LAYOUT`], fields separated with `|`.
    const LIST_PANES: &str = indoc! {"
        demo|@1|editor|0|tiled-ish|/h/code|tig|tig|git|host
        demo|@1|editor|0|tiled-ish|/src||nvim|host|host
        demo|@2|notes|0|single|/h/code||zsh|host|host
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

    fn list(dir: &Path, table: bool, tmux: &FakeTmux) -> String {
        let writer = Writer::new();
        let program = Program::parse_from(["tmux-layout", "list", "-d", dir.to_str().unwrap()]);
        let ProgramCommand::List(args) = program.command else {
            unreachable!()
        };
        let mut command = ListCommand {
            writer: Box::new(writer.clone()),
            table,
            width: Some(60),
            tmux: Box::new(tmux.clone()),
            environment: env(&[]),
        };
        command.execute(&args).unwrap();
        writer.contents()
    }

    #[test]
    fn list_prints_the_layout_names() {
        let dir = layout_dir(LAYOUT);
        std::fs::write(dir.path().join("work.yaml"), "").unwrap();
        let tmux = FakeTmux::default();
        assert_eq!(list(dir.path(), false, &tmux), "demo\nwork\n");
        assert!(tmux.calls().is_empty());
    }

    #[test]
    fn list_prints_a_table_of_the_layouts() {
        let dir = layout_dir(LAYOUT);
        std::fs::write(dir.path().join("broken.yml"), "windows: [{}]").unwrap();
        let tmux = FakeTmux::existing("demo");
        assert_eq!(
            list(dir.path(), true, &tmux),
            indoc! {"
                  LAYOUT  SESSION  WINDOWS  PANES  CWD
                  broken  —              —      —  ✗ session.name is requir…
                ● demo    demo           2      3  ~/code

                2 layouts, 1 running
            "}
        );
    }

    #[test]
    fn list_prints_a_table_without_a_tmux_server() {
        let dir = layout_dir(LAYOUT);
        let tmux = FakeTmux {
            server: false,
            ..FakeTmux::existing("demo")
        };
        let table = list(dir.path(), true, &tmux);
        assert!(table.contains("  demo    demo"));
        assert!(table.ends_with("\n1 layout\n"));
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
            confirm: None,
        };
        command.execute(&args)
    }

    /// Edits the layout `name` like [`edit`], answering whether to re-open it with `answer`.
    /// Returns the result and how often it was asked.
    fn edit_answering(dir: &Path, name: &str, editor: &str, answer: bool) -> (Result<()>, usize) {
        let dir = dir.to_str().unwrap();
        let program = Program::parse_from(["tmux-layout", "edit", "-d", dir, name]);
        let ProgramCommand::Edit(args) = program.command else {
            unreachable!()
        };
        let asked = Rc::new(Cell::new(0));
        let counter = asked.clone();
        let mut command = EditCommand {
            editor: editor.to_string(),
            environment: env(&[]),
            confirm: Some(Box::new(move |_| {
                counter.set(counter.get() + 1);
                Ok(answer)
            })),
        };
        (command.execute(&args), asked.get())
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

    fn save(tmux: &FakeTmux, dir: &Path, argv: &[&str], env: &[(&str, &str)]) -> Result<()> {
        let dir = dir.to_str().unwrap();
        let argv = [&["tmux-layout", "save", "-d", dir], argv].concat();
        let ProgramCommand::Save(args) = Program::parse_from(argv).command else {
            unreachable!()
        };
        let mut command = SaveCommand {
            tmux: Box::new(tmux.clone()),
            environment: self::env(env),
        };
        command.execute(&args)
    }

    const IN_TMUX: [(&str, &str); 2] = [("TMUX", "/tmp/tmux-501/default,1,0"), ("HOME", "/h")];

    #[test]
    fn save_writes_the_current_session_inside_tmux() {
        let dir = tempfile::tempdir().unwrap();
        let tmux = FakeTmux::default();
        save(&tmux, dir.path(), &["copy"], &IN_TMUX).unwrap();
        assert_eq!(
            tmux.calls(),
            vec![format!("list-panes -s -F {PANE_FORMAT}")]
        );

        let text = std::fs::read_to_string(dir.path().join("copy.yml")).unwrap();
        assert!(text.starts_with("# Open this layout with `tmux-layout switch copy`."));
        let expected = Layout::parse(&LAYOUT.replace("tiled", "tiled-ish")).unwrap();
        assert_eq!(Layout::parse(&text).unwrap(), expected);
    }

    #[test]
    fn save_reads_the_target_session() {
        let dir = tempfile::tempdir().unwrap();
        let tmux = FakeTmux::default();
        save(&tmux, dir.path(), &["copy", "-t", "work"], &[]).unwrap();
        assert_eq!(
            tmux.calls(),
            vec![format!("list-panes -s -F {PANE_FORMAT} -t work")]
        );
    }

    #[test]
    fn save_fails_outside_tmux_without_a_target() {
        let dir = tempfile::tempdir().unwrap();
        let tmux = FakeTmux::default();
        let err = save(&tmux, dir.path(), &["copy"], &[]).unwrap_err();
        assert!(err.to_string().contains("not inside tmux"));
        assert!(err.to_string().contains("--target"));
        assert!(tmux.calls().is_empty());
    }

    #[test]
    fn save_replaces_a_layout_only_with_force() {
        let dir = layout_dir("mine");
        let tmux = FakeTmux::default();
        let err = save(&tmux, dir.path(), &["demo"], &IN_TMUX).unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert!(tmux.calls().is_empty());

        save(&tmux, dir.path(), &["demo", "--force"], &IN_TMUX).unwrap();
        let text = std::fs::read_to_string(dir.path().join("demo.yml")).unwrap();
        assert!(text.contains("session:"));
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
    fn edit_reopens_an_invalid_layout_until_it_is_valid() {
        let dir = layout_dir(LAYOUT);
        let (invalid, valid) = (dir.path().join("invalid"), dir.path().join("valid"));
        std::fs::write(&invalid, "windows: [{}]").unwrap();
        std::fs::write(&valid, LAYOUT).unwrap();

        // The editor "saves" the invalid layout the first time, and a valid one the next
        let opened = dir.path().join("opened");
        let editor = format!(
            "f() {{ if [ -e '{0}' ]; then cp '{1}' \"$1\"; else touch '{0}'; cp '{2}' \"$1\"; fi; }}; f",
            opened.display(),
            valid.display(),
            invalid.display()
        );
        let (result, asked) = edit_answering(dir.path(), "demo", &editor, true);
        result.unwrap();
        assert_eq!(asked, 1);
    }

    #[test]
    fn edit_fails_when_reopening_is_declined() {
        let dir = layout_dir(LAYOUT);
        let invalid = dir.path().join("invalid");
        std::fs::write(&invalid, "windows: [{}]").unwrap();

        let editor = format!("cp '{}'", invalid.display());
        let (result, asked) = edit_answering(dir.path(), "demo", &editor, false);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("demo.yml is invalid"));
        assert!(err.contains("fix it with `tmux-layout edit demo`"));
        assert_eq!(asked, 1);
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
