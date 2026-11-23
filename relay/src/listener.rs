//! Run a `pilot_inference` listener as a child process of the relay.
//!
//! Starting a Flight has meant a terminal: `cd inference`, then a `uv run
//! python -m ...` line with the pilot and flight ids typed correctly, in the
//! right order relative to the relay and X-Plane (#285). This module lets the
//! relay window do it instead, and show the listener's output next to the
//! connection status it already has.
//!
//! The relay must outlive the listener (`docs/adr/0005`). The plugin cannot
//! reconnect to a relay that has restarted, so a listener starting, stopping
//! or crashing has to leave the relay itself untouched. Nothing here touches
//! the IPC or TCP threads.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use crossbeam::channel::{unbounded, Receiver, Sender};

use crate::events::{parse_event, protocol_version, ListenerEvent, Status, PROTOCOL_VERSION};

/// Which listener the panel runs.
///
/// They are alternatives, not both: each binds the mirror port, and the
/// second would fail to bind while the first kept the Flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListenerMode {
    /// Record the Flight to a CSV.
    Capture,
    /// Score the Flight against a brief and name a performance group.
    LiveGroups,
}

impl ListenerMode {
    pub(crate) const ALL: [ListenerMode; 2] = [ListenerMode::Capture, ListenerMode::LiveGroups];
}

impl std::fmt::Display for ListenerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ListenerMode::Capture => "Capture",
            ListenerMode::LiveGroups => "Live groups",
        })
    }
}

/// The port the relay's mirror writes to, and so the one a listener binds.
pub(crate) const DEFAULT_LISTENER_PORT: u16 = 5001;

/// Lines of listener output kept for the window. Older lines are dropped.
const OUTPUT_LINES_KEPT: usize = 200;

/// How long a listener gets to close its file after being asked to stop,
/// before it is killed.
const STOP_GRACE: Duration = Duration::from_millis(1500);

/// Characters allowed in a pilot or flight id. Both end up in a filename and
/// in every row of the CSV, so the set is deliberately narrow.
fn is_allowed_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

/// Longest id accepted. Well under any filename limit, and long enough for
/// the `<pilot>-<date>-<time>` shape the launcher issue suggests.
const MAX_ID_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ListenerStatus {
    /// Never started, or stopped and reaped.
    Stopped,
    /// Started and still running.
    Running,
    /// Exited on its own. A listener that ends by itself mid-Flight is worth
    /// seeing, so the code is kept rather than folded into `Stopped`.
    Finished(Option<i32>),
    /// Could not be started, or died in a way worth reporting.
    Failed(String),
}

impl ListenerStatus {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self, ListenerStatus::Running)
    }

    /// One line for the window.
    pub(crate) fn describe(&self) -> String {
        match self {
            ListenerStatus::Stopped => "Listener: not running".to_string(),
            ListenerStatus::Running => "Listener: running".to_string(),
            ListenerStatus::Finished(Some(0)) => "Listener: finished".to_string(),
            ListenerStatus::Finished(Some(code)) => format!("Listener: exited with code {code}"),
            ListenerStatus::Finished(None) => "Listener: stopped".to_string(),
            ListenerStatus::Failed(reason) => format!("Listener: {reason}"),
        }
    }
}

/// A command to run, built and checked before anything is spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListenerCommand {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) working_dir: PathBuf,
}

impl ListenerCommand {
    /// What the operator would have typed, for the event log.
    pub(crate) fn display(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

/// Check one id and hand back the trimmed form.
///
/// The id reaches the child as a single argv entry, so a shell is never
/// involved and quoting is not the concern. It does become part of the CSV's
/// name, which is why a slash or a `..` is refused here rather than being
/// discovered as a file written somewhere unexpected.
pub(crate) fn checked_id(value: &str, what: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{what} is required");
    }
    if trimmed.len() > MAX_ID_LEN {
        bail!("{what} is longer than {MAX_ID_LEN} characters");
    }
    if let Some(bad) = trimmed.chars().find(|c| !is_allowed_id_char(*c)) {
        bail!("{what} cannot contain {bad:?}. Use letters, digits, dot, dash or underscore.");
    }
    Ok(trimmed.to_string())
}

/// Build the command that runs the capture logger.
///
/// `python -u` matters. Python block-buffers stdout when it is a pipe rather
/// than a console, so without it the listener's output would reach the window
/// in silent chunks of a few kilobytes, or not until it exited.
pub(crate) fn capture_command(
    inference_dir: &Path,
    pilot_id: &str,
    flight_id: &str,
    port: u16,
) -> Result<ListenerCommand> {
    let pilot_id = checked_id(pilot_id, "Pilot id")?;
    let flight_id = checked_id(flight_id, "Flight id")?;
    if !inference_dir.join("pyproject.toml").is_file() {
        bail!(
            "{} is not the inference project. Point the relay at the folder that \
             holds pyproject.toml.",
            inference_dir.display()
        );
    }
    Ok(ListenerCommand {
        program: "uv".to_string(),
        args: vec![
            "run".into(),
            "python".into(),
            "-u".into(),
            "-m".into(),
            "pilot_inference.capture.data_logger".into(),
            "--pilot-id".into(),
            pilot_id,
            "--flight-id".into(),
            flight_id,
            "--port".into(),
            port.to_string(),
        ],
        working_dir: inference_dir.to_path_buf(),
    })
}

/// Build the command that runs the live performance groups sidecar.
///
/// `--json` is what makes the panel possible: the sidecar reports events
/// instead of console sentences, so the window shows the group and the
/// instruction progress as data rather than scraped text.
pub(crate) fn live_groups_command(
    inference_dir: &Path,
    brief: &Path,
    pilot_id: &str,
    flight_id: &str,
    port: u16,
) -> Result<ListenerCommand> {
    let pilot_id = checked_id(pilot_id, "Pilot id")?;
    let flight_id = checked_id(flight_id, "Flight id")?;
    if !inference_dir.join("pyproject.toml").is_file() {
        bail!(
            "{} is not the inference project. Point the relay at the folder that              holds pyproject.toml.",
            inference_dir.display()
        );
    }
    // A relative brief is relative to the inference project, because that is
    // where the child runs and where briefs/ lives. Resolving it here means
    // this check and the child's own open() look at the same file.
    let brief = if brief.is_absolute() {
        brief.to_path_buf()
    } else {
        inference_dir.join(brief)
    };
    if !brief.is_file() {
        bail!(
            "No brief at {}. A live groups flight needs one: it is the list of              instructions and the targets each one sets.",
            brief.display()
        );
    }
    Ok(ListenerCommand {
        program: "uv".to_string(),
        args: vec![
            "run".into(),
            "python".into(),
            "-u".into(),
            "-m".into(),
            "pilot_inference.performance.live_groups".into(),
            "--json".into(),
            "--brief".into(),
            brief.to_string_lossy().into_owned(),
            "--pilot-id".into(),
            pilot_id,
            "--flight-id".into(),
            flight_id,
            "--port".into(),
            port.to_string(),
        ],
        working_dir: inference_dir.to_path_buf(),
    })
}

/// What the live groups sidecar has reported so far.
///
/// Held as data rather than as lines of text, so the window can put the group
/// where the Instructor is looking instead of at the end of a log.
#[derive(Debug, Default, Clone)]
pub(crate) struct LiveState {
    pub(crate) status: Option<Status>,
    pub(crate) cohort: Option<String>,
    pub(crate) agreement: Option<f64>,
    pub(crate) pilot_id: String,
    pub(crate) flight_id: String,
    pub(crate) csv: String,
    pub(crate) given: u32,
    pub(crate) total: u32,
    pub(crate) next_instruction: Option<String>,
    pub(crate) last_instruction: Option<String>,
    pub(crate) note: Option<String>,
}

impl LiveState {
    /// True once the sidecar has said it is listening, which is when giving an
    /// instruction starts to mean something.
    pub(crate) fn ready(&self) -> bool {
        !self.flight_id.is_empty()
    }

    fn apply(&mut self, event: ListenerEvent) {
        match event {
            ListenerEvent::Fitting { k, cohort_pilots } => {
                self.note = Some(format!(
                    "Fitting {k} groups on {cohort_pilots} synthetic pilots..."
                ));
            }
            ListenerEvent::Cohort {
                agreement,
                description,
            } => {
                self.agreement = Some(agreement);
                self.cohort = Some(description);
                self.note = None;
            }
            ListenerEvent::Started {
                pilot_id,
                flight_id,
                csv,
                total,
                next,
                unnamed_pilot,
                ..
            } => {
                self.pilot_id = pilot_id;
                self.flight_id = flight_id;
                self.csv = csv;
                self.total = total;
                self.next_instruction = next;
                if unnamed_pilot {
                    self.note = Some(
                        "No pilot id was given, so these rows cannot be grouped per pilot."
                            .to_string(),
                    );
                }
            }
            ListenerEvent::Status(status) => self.status = Some(status),
            ListenerEvent::Instruction {
                given, total, text, next,
            } => {
                self.given = given;
                self.total = total;
                self.last_instruction = Some(text);
                self.next_instruction = next;
            }
            ListenerEvent::Stopped {
                scored_seconds,
                given,
            } => {
                self.note = Some(format!(
                    "Stopped. {scored_seconds:.0}s scored over {given} instruction(s)."
                ));
                self.next_instruction = None;
            }
            ListenerEvent::Error { message } => self.note = Some(message),
        }
    }
}

/// Find the `inference/` folder from where the relay is running.
///
/// `relay.exe` runs from `relay/target/release/` in a dev tree and from
/// wherever someone saved it after downloading a release, so the folder is
/// searched for rather than assumed. Returns `None` when it is not nearby,
/// which the panel reports instead of failing at spawn time.
pub(crate) fn find_inference_dir() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }

    for root in roots {
        let mut candidate = root.as_path();
        for _ in 0..6 {
            let inference = candidate.join("inference");
            if inference.join("pyproject.toml").is_file() {
                return Some(inference);
            }
            match candidate.parent() {
                Some(parent) => candidate = parent,
                None => break,
            }
        }
    }
    None
}

/// A listener the relay started, and the output it has produced.
pub(crate) struct Listener {
    child: Option<Child>,
    stdin: Option<std::process::ChildStdin>,
    output_rx: Option<Receiver<String>>,
    pub(crate) status: ListenerStatus,
    pub(crate) output: VecDeque<String>,
    pub(crate) command: Option<String>,
    pub(crate) live: LiveState,
    version_warned: bool,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            child: None,
            stdin: None,
            output_rx: None,
            status: ListenerStatus::Stopped,
            output: VecDeque::new(),
            command: None,
            live: LiveState::default(),
            version_warned: false,
        }
    }
}

impl Listener {
    /// Start a listener. An already running one is left alone and reported,
    /// rather than being replaced, because a second logger on the same port
    /// would fail to bind and the first would keep the Flight.
    pub(crate) fn start(&mut self, command: ListenerCommand) -> Result<()> {
        if self.status.is_running() {
            bail!("A listener is already running. Stop it before starting another.");
        }

        let mut child = Command::new(&command.program)
            .args(&command.args)
            .current_dir(&command.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => anyhow!(
                    "{} is not on PATH. The listener runs under uv; install it, or start \
                     the listener from a terminal as before.",
                    command.program
                ),
                _ => anyhow!("Could not start {}: {e}", command.program),
            })?;

        let (tx, rx) = unbounded();
        if let Some(stdout) = child.stdout.take() {
            pump(stdout, tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            pump(stderr, tx);
        }

        self.stdin = child.stdin.take();
        self.output.clear();
        self.live = LiveState::default();
        self.version_warned = false;
        self.command = Some(command.display());
        self.child = Some(child);
        self.output_rx = Some(rx);
        self.status = ListenerStatus::Running;
        Ok(())
    }

    /// Collect whatever the listener has written and notice if it has exited.
    ///
    /// Called from the relay's existing tick, so the window keeps up with the
    /// listener without a second subscription.
    pub(crate) fn poll(&mut self) {
        let lines: Vec<String> = match &self.output_rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for line in lines {
            // A `--json` listener reports events; anything else is uv's own
            // progress or a traceback, and belongs in the output pane.
            match parse_event(&line) {
                Some(event) => {
                    if !self.version_warned {
                        if let Some(version) = protocol_version(&line) {
                            if version != PROTOCOL_VERSION {
                                self.version_warned = true;
                                self.push_output(format!(
                                    "Listener speaks event protocol v{version}, this relay                                      reads v{PROTOCOL_VERSION}. Fields it has renamed will                                      be missing."
                                ));
                            }
                        }
                    }
                    self.live.apply(event);
                }
                None => self.push_output(line),
            }
        }

        let exited = match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => Some(ListenerStatus::Finished(status.code())),
                Ok(None) => None,
                Err(e) => Some(ListenerStatus::Failed(format!("could not be checked: {e}"))),
            },
            None => None,
        };

        if let Some(status) = exited {
            self.child = None;
            self.stdin = None;
            self.status = status;
        }
    }

    /// Ask the listener to stop, then make sure it has.
    ///
    /// `q` and a closed stdin is how `live_groups` ends: it writes everything
    /// it has and closes its file. `data_logger` does not read stdin yet
    /// (#277), so it is killed after the grace period. It flushes every row as
    /// it arrives, so that costs at most the sample in flight.
    pub(crate) fn stop(&mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.write_all(b"q\n");
            let _ = stdin.flush();
            // Dropping it closes the pipe, so a listener blocked on a read
            // sees EOF even if it ignored the `q`.
        }

        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline {
            self.poll();
            if self.child.is_none() {
                self.status = ListenerStatus::Stopped;
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }

        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.child = None;
        self.status = ListenerStatus::Stopped;
    }

    fn push_output(&mut self, line: String) {
        if self.output.len() == OUTPUT_LINES_KEPT {
            self.output.pop_front();
        }
        self.output.push_back(line);
    }

    /// Send one line to the listener's stdin.
    ///
    /// An empty line is how `live_groups` is told an instruction was just
    /// given: it reads a line per instruction, exactly as pressing Enter at a
    /// console does.
    pub(crate) fn send_line(&mut self, line: &str) -> Result<()> {
        let Some(stdin) = self.stdin.as_mut() else {
            bail!("No listener is running, so there is nothing to tell.");
        };
        stdin
            .write_all(format!("{line}
").as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| anyhow!("The listener stopped reading: {e}"))
    }

    /// The most recent lines, oldest first, for the window.
    pub(crate) fn recent_output(&self, count: usize) -> Vec<&str> {
        let skip = self.output.len().saturating_sub(count);
        self.output.iter().skip(skip).map(String::as_str).collect()
    }
}

/// Forward one of the child's streams into the channel, a line at a time.
fn pump<R: std::io::Read + Send + 'static>(stream: R, tx: Sender<String>) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inference_dir() -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("relay/ has a parent")
            .join("inference");
        assert!(dir.join("pyproject.toml").is_file(), "test needs inference/");
        dir
    }

    /// A trivial child that stays alive until its stdin closes, so the
    /// lifecycle can be exercised without uv or Python.
    fn sleeper() -> ListenerCommand {
        let (program, args) = if cfg!(windows) {
            ("cmd", vec!["/C".to_string(), "echo started& pause".to_string()])
        } else {
            ("sh", vec!["-c".to_string(), "echo started; read _".to_string()])
        };
        ListenerCommand {
            program: program.to_string(),
            args,
            working_dir: std::env::temp_dir(),
        }
    }

    #[test]
    fn an_id_keeps_the_characters_a_filename_can_hold() {
        assert_eq!(checked_id(" P004 ", "Pilot id").unwrap(), "P004");
        assert_eq!(checked_id("P004-2026-11-23.1", "Flight id").unwrap(), "P004-2026-11-23.1");
    }

    #[test]
    fn an_empty_id_is_refused() {
        let error = checked_id("   ", "Pilot id").unwrap_err().to_string();
        assert!(error.contains("Pilot id is required"), "{error}");
    }

    #[test]
    fn an_id_that_would_escape_the_data_folder_is_refused() {
        for bad in ["../secrets", "a/b", "a\\b", "a:b"] {
            assert!(checked_id(bad, "Flight id").is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn an_overlong_id_is_refused() {
        assert!(checked_id(&"P".repeat(MAX_ID_LEN + 1), "Pilot id").is_err());
    }

    #[test]
    fn the_capture_command_runs_the_logger_unbuffered() {
        let command = capture_command(&inference_dir(), "P004", "P004-T1", 5001).unwrap();
        assert_eq!(command.program, "uv");
        assert!(command.args.contains(&"-u".to_string()), "python must not buffer its output");
        assert!(command
            .args
            .contains(&"pilot_inference.capture.data_logger".to_string()));
    }

    #[test]
    fn the_capture_command_passes_the_ids_and_the_port() {
        let command = capture_command(&inference_dir(), " P004 ", "P004-T1", 5002).unwrap();
        let rendered = command.display();
        assert!(rendered.contains("--pilot-id P004"), "{rendered}");
        assert!(rendered.contains("--flight-id P004-T1"), "{rendered}");
        assert!(rendered.contains("--port 5002"), "{rendered}");
    }

    #[test]
    fn a_bad_id_stops_the_command_being_built_at_all() {
        assert!(capture_command(&inference_dir(), "", "P004-T1", 5001).is_err());
        assert!(capture_command(&inference_dir(), "P004", "../x", 5001).is_err());
    }

    #[test]
    fn a_folder_without_pyproject_is_refused_with_a_usable_message() {
        let error = capture_command(std::env::temp_dir().as_path(), "P004", "T1", 5001)
            .unwrap_err()
            .to_string();
        assert!(error.contains("pyproject.toml"), "{error}");
    }

    #[test]
    fn the_inference_folder_is_found_from_the_test_binary() {
        // The test binary lives in relay/target/debug/deps, so this also
        // covers the depth relay.exe runs at in a dev tree.
        assert!(find_inference_dir().is_some());
    }

    #[test]
    fn a_missing_program_reports_what_is_missing() {
        let mut listener = Listener::default();
        let error = listener
            .start(ListenerCommand {
                program: "definitely-not-on-path-xyzzy".into(),
                args: vec![],
                working_dir: std::env::temp_dir(),
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("not on PATH"), "{error}");
        assert_eq!(listener.status, ListenerStatus::Stopped);
    }

    #[test]
    fn a_started_listener_runs_and_its_output_reaches_the_window() {
        let mut listener = Listener::default();
        listener.start(sleeper()).unwrap();
        assert!(listener.status.is_running());

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && listener.output.is_empty() {
            listener.poll();
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            listener.recent_output(5).iter().any(|l| l.contains("started")),
            "output was {:?}",
            listener.output
        );

        listener.stop();
        assert_eq!(listener.status, ListenerStatus::Stopped);
    }

    #[test]
    fn stopping_a_listener_that_was_never_started_is_harmless() {
        let mut listener = Listener::default();
        listener.stop();
        assert_eq!(listener.status, ListenerStatus::Stopped);
    }

    #[test]
    fn a_second_listener_is_refused_while_one_is_running() {
        let mut listener = Listener::default();
        listener.start(sleeper()).unwrap();
        let error = listener.start(sleeper()).unwrap_err().to_string();
        assert!(error.contains("already running"), "{error}");
        listener.stop();
    }

    #[test]
    fn the_output_buffer_does_not_grow_without_limit() {
        let mut listener = Listener::default();
        for i in 0..(OUTPUT_LINES_KEPT * 2) {
            if listener.output.len() == OUTPUT_LINES_KEPT {
                listener.output.pop_front();
            }
            listener.output.push_back(format!("line {i}"));
        }
        assert_eq!(listener.output.len(), OUTPUT_LINES_KEPT);
        assert_eq!(listener.recent_output(1), vec![format!(
            "line {}",
            OUTPUT_LINES_KEPT * 2 - 1
        )]);
    }

    #[test]
    fn recent_output_asks_for_more_than_there_is() {
        let mut listener = Listener::default();
        listener.output.push_back("only line".to_string());
        assert_eq!(listener.recent_output(10), vec!["only line"]);
    }

    #[test]
    fn the_live_groups_command_asks_for_json_and_the_brief() {
        let dir = inference_dir();
        let brief = dir.join("briefs").join("example_level_holds.csv");
        let command = live_groups_command(&dir, &brief, "P004", "P004-T1", 5001).unwrap();
        let rendered = command.display();
        assert!(rendered.contains("--json"), "{rendered}");
        assert!(rendered.contains("pilot_inference.performance.live_groups"), "{rendered}");
        assert!(rendered.contains("example_level_holds.csv"), "{rendered}");
        assert!(command.args.contains(&"-u".to_string()));
    }

    #[test]
    fn a_relative_brief_is_found_inside_the_inference_project() {
        // The child runs with inference/ as its working directory, so a
        // relative path has to mean the same file on both sides.
        let command = live_groups_command(
            &inference_dir(),
            Path::new("briefs/example_level_holds.csv"),
            "P004",
            "T1",
            5001,
        )
        .unwrap();
        assert!(command.display().contains("example_level_holds.csv"));
    }

    #[test]
    fn a_missing_brief_is_refused_before_anything_spawns() {
        let dir = inference_dir();
        let error = live_groups_command(&dir, &dir.join("no_such_brief.csv"), "P004", "T1", 5001)
            .unwrap_err()
            .to_string();
        assert!(error.contains("No brief at"), "{error}");
    }

    #[test]
    fn live_groups_checks_the_ids_like_capture_does() {
        let dir = inference_dir();
        let brief = dir.join("briefs").join("example_level_holds.csv");
        assert!(live_groups_command(&dir, &brief, "", "T1", 5001).is_err());
        assert!(live_groups_command(&dir, &brief, "P004", "../x", 5001).is_err());
    }

    #[test]
    fn telling_a_listener_that_is_not_running_is_an_error_not_a_panic() {
        let mut listener = Listener::default();
        assert!(listener.send_line("").is_err());
    }

    #[test]
    fn events_become_live_state_and_other_lines_become_output() {
        let mut listener = Listener::default();
        let (tx, rx) = unbounded();
        listener.output_rx = Some(rx);

        tx.send("Installed 1 package in 10ms".to_string()).unwrap();
        tx.send(
            r#"{"event":"started","v":1,"pilot_id":"P004","flight_id":"P004-T1","total":4,"next":"Fly heading 270","csv":"live.csv","port":5001}"#
                .to_string(),
        )
        .unwrap();
        tx.send(
            r#"{"event":"status","v":1,"state":"scoring","given":2,"total":4,"scored_seconds":92.0,"min_scored":60.0,"provisional":false,"group":1}"#
                .to_string(),
        )
        .unwrap();
        drop(tx);
        listener.poll();

        assert_eq!(listener.live.pilot_id, "P004");
        assert_eq!(listener.live.flight_id, "P004-T1");
        assert_eq!(listener.live.total, 4);
        assert!(listener.live.ready());
        assert_eq!(
            listener.live.status.as_ref().unwrap().headline(),
            "Instruction 2/4, scored 92s, group 1"
        );
        // The JSON never reaches the output pane, but uv's chatter does.
        assert_eq!(listener.recent_output(10), vec!["Installed 1 package in 10ms"]);
    }

    #[test]
    fn an_instruction_event_moves_the_brief_on() {
        let mut listener = Listener::default();
        let (tx, rx) = unbounded();
        listener.output_rx = Some(rx);
        tx.send(
            r#"{"event":"instruction","v":1,"given":1,"total":4,"text":"Fly heading 270","next":"Turn right heading 360"}"#
                .to_string(),
        )
        .unwrap();
        drop(tx);
        listener.poll();

        assert_eq!(listener.live.given, 1);
        assert_eq!(listener.live.last_instruction.as_deref(), Some("Fly heading 270"));
        assert_eq!(
            listener.live.next_instruction.as_deref(),
            Some("Turn right heading 360")
        );
    }

    #[test]
    fn a_listener_speaking_a_different_protocol_says_so_once() {
        let mut listener = Listener::default();
        let (tx, rx) = unbounded();
        listener.output_rx = Some(rx);
        for _ in 0..3 {
            tx.send(r#"{"event":"status","v":99,"state":"waiting"}"#.to_string())
                .unwrap();
        }
        drop(tx);
        listener.poll();

        let warnings: Vec<_> = listener
            .recent_output(10)
            .into_iter()
            .filter(|l| l.contains("event protocol"))
            .collect();
        assert_eq!(warnings.len(), 1, "warned {} times", warnings.len());
    }

    #[test]
    fn starting_again_clears_the_previous_flight() {
        let mut listener = Listener::default();
        listener.live.flight_id = "OLD".into();
        listener.start(sleeper()).unwrap();
        assert_eq!(listener.live.flight_id, "");
        listener.stop();
    }

    /// The panel driving the real sidecar: spawn it, read its events, and
    /// give it an instruction.
    ///
    /// Ignored because it needs uv and spends about fifteen seconds fitting
    /// the reference cohort, which does not belong in the ordinary run. Run
    /// it with `cargo test -- --ignored --nocapture` after changing either
    /// side of the event contract.
    #[test]
    #[ignore = "needs uv and a cohort fit; run manually"]
    fn the_real_live_groups_sidecar_reports_and_takes_an_instruction() {
        let dir = inference_dir();
        let command = live_groups_command(
            &dir,
            Path::new("briefs/example_level_holds.csv"),
            "P004",
            "RUSTE2E",
            5099,
        )
        .unwrap();

        let mut listener = Listener::default();
        listener.start(command).unwrap();

        let deadline = Instant::now() + Duration::from_secs(90);
        while Instant::now() < deadline && !listener.live.ready() {
            listener.poll();
            thread::sleep(Duration::from_millis(100));
        }
        assert!(
            listener.live.ready(),
            "the sidecar never reported started; output was {:?}",
            listener.output
        );
        assert_eq!(listener.live.flight_id, "RUSTE2E");
        assert_eq!(listener.live.pilot_id, "P004");
        assert_eq!(listener.live.total, 4);
        assert!(listener.live.agreement.is_some(), "no cohort event arrived");
        assert_eq!(
            listener.live.next_instruction.as_deref(),
            Some("Fly heading 270, maintain 3000 feet, 100 knots")
        );

        // This is what the Give instruction button does.
        listener.send_line("").unwrap();

        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && listener.live.given == 0 {
            listener.poll();
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(listener.live.given, 1, "the instruction was not taken");
        assert_eq!(
            listener.live.next_instruction.as_deref(),
            Some("Turn right heading 360")
        );

        // And a status line should arrive on its own.
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && listener.live.status.is_none() {
            listener.poll();
            thread::sleep(Duration::from_millis(100));
        }
        let status = listener.live.status.as_ref().expect("no status event arrived");
        assert!(status.headline().starts_with("Instruction 1/4"), "{}", status.headline());

        listener.stop();
        assert_eq!(listener.status, ListenerStatus::Stopped);
    }

    #[test]
    fn the_status_line_reads_as_a_sentence() {
        assert_eq!(ListenerStatus::Stopped.describe(), "Listener: not running");
        assert_eq!(ListenerStatus::Running.describe(), "Listener: running");
        assert_eq!(ListenerStatus::Finished(Some(2)).describe(), "Listener: exited with code 2");
        assert!(ListenerStatus::Failed("uv is not on PATH".into())
            .describe()
            .contains("uv is not on PATH"));
    }
}
