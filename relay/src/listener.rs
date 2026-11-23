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
        if let Some(rx) = &self.output_rx {
            for line in rx.try_iter() {
                if self.output.len() == OUTPUT_LINES_KEPT {
                    self.output.pop_front();
                }
                self.output.push_back(line);
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
    fn the_status_line_reads_as_a_sentence() {
        assert_eq!(ListenerStatus::Stopped.describe(), "Listener: not running");
        assert_eq!(ListenerStatus::Running.describe(), "Listener: running");
        assert_eq!(ListenerStatus::Finished(Some(2)).describe(), "Listener: exited with code 2");
        assert!(ListenerStatus::Failed("uv is not on PATH".into())
            .describe()
            .contains("uv is not on PATH"));
    }
}
