//! Replay a recorded flight into the relay, so a Flight needs no simulator.
//!
//! The point of the window is that running a session needs no terminal, and
//! that includes checking the chain works before a pilot sits down. There was
//! a replay already, `pilot_inference.capture.replay`, but it is a command
//! line and it connects straight to the listener, which bypasses the relay and
//! so tests none of it.
//!
//! This one enters where baton's samples enter, as `ToTcpThreadMessage::Send`
//! with a payload in baton's own shape. iMotions, the mirror and the listener
//! all see a live flight, and nothing downstream knows the difference. That is
//! what makes it worth having: it exercises the real path rather than a
//! shortcut around it.
//!
//! Rows are sent as they are. The recordings in `inference/data/` predate #196,
//! so their altitude and velocity are mis-scaled
//! (`docs/telemetry_schema.md`); replaying one sends those wrong values
//! exactly as the plugin of the day did, which is the honest thing for a
//! replay to do.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

/// Samples a second. The plugin sends at 20 Hz (`docs/telemetry_schema.md`),
/// and the scoring in `live_groups` measures seconds of flight, so a replay
/// has to run at the real rate rather than as fast as it can.
pub(crate) const DEFAULT_REPLAY_HZ: f64 = 20.0;

/// The telemetry the relay carries, in the order its packets go out
/// (`IMOTIONS_EVENTS` in `state.rs`). A column a recording lacks is sent as 0:
/// synthetic telemetry has no roll, pitch, yaw or g_force.
const REPLAY_COLUMNS: [&str; 8] = [
    "altitude",
    "velocity",
    "vertical_speed",
    "heading",
    "roll",
    "pitch",
    "yaw",
    "g_force",
];

/// Header spellings that mean the same column. `velocity` is the plugin's name
/// for airspeed and `g_force` is written both ways, so a recording from either
/// side of the project replays without being edited first.
fn column_aliases(column: &str) -> &'static [&'static str] {
    match column {
        "velocity" => &["velocity", "airspeed", "indicated_airspeed"],
        "vertical_speed" => &["vertical_speed", "vertical_velocity", "vvi"],
        "g_force" => &["g_force", "gforce", "g_load"],
        "altitude" => &["altitude"],
        "heading" => &["heading"],
        "roll" => &["roll"],
        "pitch" => &["pitch"],
        "yaw" => &["yaw"],
        _ => &[],
    }
}

/// Split one CSV line.
///
/// Hand-rolled rather than a crate because the relay has no CSV dependency and
/// this reads telemetry, which is numbers. Double quotes are still honoured,
/// since a recording may carry a quoted instruction column alongside them and
/// a comma inside it must not shift every column after it.
pub(crate) fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches(['\r', '\n']).chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                current.push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

/// Where each replayed column sits in a recording's header, by name rather
/// than by position, because these CSVs do not agree on column order.
fn column_positions(header: &[String]) -> Vec<Option<usize>> {
    let lowered: Vec<String> = header
        .iter()
        .map(|name| name.trim().to_ascii_lowercase())
        .collect();
    REPLAY_COLUMNS
        .iter()
        .map(|column| {
            column_aliases(column)
                .iter()
                .find_map(|alias| lowered.iter().position(|name| name == alias))
        })
        .collect()
}

/// One sample as baton would have sent it.
///
/// Every value goes out twice, flightmodel half then pilot half. A recording
/// holds pilot readings only, so the flightmodel half is the same number
/// standing in for the one that was never recorded (#184). Consumers that want
/// the pilot reading take the second value, as `data_logger.parse_packet`
/// does, so the duplication costs them nothing.
fn payload_for_row(row: &[String], positions: &[Option<usize>]) -> String {
    let mut fields: Vec<String> = Vec::with_capacity(REPLAY_COLUMNS.len() * 2);
    for position in positions {
        let value = position
            .and_then(|index| row.get(index))
            .map(|raw| raw.trim())
            .filter(|raw| !raw.is_empty())
            .and_then(|raw| raw.parse::<f64>().ok())
            .unwrap_or(0.0);
        let rendered = format!("{value:.6}");
        fields.push(rendered.clone());
        fields.push(rendered);
    }
    fields.join(";")
}

/// A recording the window can offer, and the flights inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Recording {
    pub(crate) path: PathBuf,
    pub(crate) flights: Vec<String>,
}

impl Recording {
    /// The name shown in the picker. The folder is the same for all of them,
    /// so the file name alone identifies it and fits the window.
    pub(crate) fn label(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

/// A recording's header, without reading the rest of it.
///
/// The committed recordings run to a hundred thousand rows, and the window
/// lists them at startup. Reading one line to decide whether a file is
/// telemetry keeps that off the startup path.
fn telemetry_header(path: &Path) -> Result<Vec<String>> {
    let file = fs::File::open(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    let mut line = String::new();
    BufReader::new(file)
        .read_line(&mut line)
        .with_context(|| format!("Could not read the header of {}", path.display()))?;
    let header = split_csv_line(&line);
    if header.is_empty() || header.iter().all(|name| name.trim().is_empty()) {
        bail!("{} is empty", path.display());
    }
    if column_positions(&header).iter().all(Option::is_none) {
        bail!(
            "{} holds no telemetry columns the relay can send. It needs at least one of \
             altitude, velocity, vertical_speed or heading.",
            path.display()
        );
    }
    Ok(header)
}

/// A recording with no flights listed yet, cheap enough for startup.
pub(crate) fn describe_recording(path: &Path) -> Result<Recording> {
    telemetry_header(path)?;
    Ok(Recording {
        path: path.to_path_buf(),
        flights: Vec::new(),
    })
}

/// Read the flight ids in a recording.
///
/// This one does read the whole file. It runs when a recording is picked, not
/// at startup and not while a flight is going out.
pub(crate) fn read_recording(path: &Path) -> Result<Recording> {
    let header = telemetry_header(path)?;
    let Some(index) = header
        .iter()
        .position(|name| name.trim().eq_ignore_ascii_case("flight_id"))
    else {
        // No flight_id column means the file holds one flight, so there is
        // nothing to choose between.
        return Ok(Recording {
            path: path.to_path_buf(),
            flights: Vec::new(),
        });
    };

    let file = fs::File::open(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    let mut flights: Vec<String> = Vec::new();
    for line in BufReader::new(file).lines().skip(1) {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let row = split_csv_line(&line);
        if let Some(id) = row.get(index) {
            let id = id.trim();
            if !id.is_empty() && !flights.iter().any(|seen| seen == id) {
                flights.push(id.to_string());
            }
        }
    }
    flights.sort();

    Ok(Recording {
        path: path.to_path_buf(),
        flights,
    })
}

/// Every CSV in `inference/data/` that holds telemetry the relay can send.
///
/// A file that does not is left out rather than offered and then refused: the
/// folder also holds tiers, labels and feature tables, and the person picking
/// one is not expected to know which is which.
pub(crate) fn recordings_in(data_dir: &Path) -> Vec<Recording> {
    let Ok(entries) = fs::read_dir(data_dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .map(|ext| ext.eq_ignore_ascii_case("csv"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();

    paths
        .iter()
        .filter_map(|path| describe_recording(path).ok())
        .collect()
}

/// Render one flight of a recording as baton payloads, ready to send.
pub(crate) fn payloads_for(path: &Path, flight_id: Option<&str>) -> Result<Vec<String>> {
    let header = telemetry_header(path)?;
    let positions = column_positions(&header);
    let flight_column = header
        .iter()
        .position(|name| name.trim().eq_ignore_ascii_case("flight_id"));

    let file = fs::File::open(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    let mut payloads = Vec::new();
    for line in BufReader::new(file).lines().skip(1) {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let row = split_csv_line(&line);
        if let (Some(wanted), Some(index)) = (flight_id, flight_column) {
            match row.get(index) {
                Some(id) if id.trim() == wanted => {}
                _ => continue,
            }
        }
        payloads.push(payload_for_row(&row, &positions));
    }

    if payloads.is_empty() {
        match flight_id {
            Some(wanted) => bail!("No rows for flight {wanted} in {}", path.display()),
            None => bail!("{} holds no rows to send", path.display()),
        }
    }
    Ok(payloads)
}

/// A replay in progress, paced from the window's own tick.
///
/// No thread. The window already ticks every 10 ms, which is fine for 20 Hz,
/// and a replay that lives entirely in the update loop cannot outlive the
/// window or race it.
#[derive(Debug, Clone)]
pub(crate) struct Replay {
    pub(crate) flight_id: String,
    payloads: Vec<String>,
    next: usize,
    period: Duration,
    start: Option<Instant>,
}

/// Most samples one `due` call will hand back.
///
/// Pacing is measured from the start so a slow tick does not stretch the
/// flight, but that alone would turn a stall into a burst of thousands of
/// samples, which is not what a 20 Hz stream looks like to anything reading
/// it. Past a second's worth the replay simply runs late.
const MAX_SAMPLES_PER_TICK: usize = 20;

impl Replay {
    pub(crate) fn new(flight_id: String, payloads: Vec<String>, hz: f64) -> Self {
        let period = if hz > 0.0 {
            Duration::from_secs_f64(1.0 / hz)
        } else {
            Duration::ZERO
        };
        Self {
            flight_id,
            payloads,
            next: 0,
            period,
            start: None,
        }
    }

    /// The payloads due by `now`, in order.
    pub(crate) fn due(&mut self, now: Instant) -> Vec<String> {
        let start = *self.start.get_or_insert(now);
        if self.next >= self.payloads.len() {
            return Vec::new();
        }

        // Counted in whole nanoseconds, not as a float division. At 20 Hz a
        // tick 150 ms in is exactly three periods, and in floating point that
        // comes out as 2.9999999, which loses a sample on every third tick.
        let wanted = if self.period.is_zero() {
            self.payloads.len()
        } else {
            let elapsed = now.saturating_duration_since(start).as_nanos();
            (elapsed / self.period.as_nanos()) as usize + 1
        };

        let limit = self
            .payloads
            .len()
            .min(wanted)
            .min(self.next + MAX_SAMPLES_PER_TICK);
        if limit <= self.next {
            return Vec::new();
        }
        let batch = self.payloads[self.next..limit].to_vec();
        self.next = limit;
        batch
    }

    pub(crate) fn sent(&self) -> usize {
        self.next
    }

    pub(crate) fn total(&self) -> usize {
        self.payloads.len()
    }

    pub(crate) fn finished(&self) -> bool {
        self.next >= self.payloads.len()
    }

    /// One line for the window.
    pub(crate) fn describe(&self) -> String {
        if self.finished() {
            format!("Replay of {} finished, {} samples", self.flight_id, self.total())
        } else {
            format!(
                "Replaying {}, sample {} of {}",
                self.flight_id,
                self.sent(),
                self.total()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of this test's own.
    ///
    /// Written rather than reaching into `inference/data/`: the recordings
    /// there are gitignored, so a test that read one would pass here and fail
    /// on a fresh clone.
    fn temp_dir(name: &str) -> PathBuf {
        // A counter as well as the name, because the tests run in parallel and
        // two of them sharing a folder is a flake rather than a failure.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir()
            .join("relay-replay-tests")
            .join(format!(
                "{name}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("could not make a temp folder");
        dir
    }

    /// Two flights, one of them three samples long.
    fn two_flight_recording(dir: &Path) -> PathBuf {
        let path = dir.join("two_flights.csv");
        fs::write(
            &path,
            "pilot_id,flight_id,altitude,heading,velocity\n\
             P01,P01-F1,3000,270,120\n\
             P01,P01-F1,3010,271,121\n\
             P01,P01-F1,3020,272,122\n\
             P02,P02-F1,5000,90,150\n",
        )
        .expect("could not write the recording");
        path
    }

    #[test]
    fn a_plain_line_splits_on_commas() {
        assert_eq!(split_csv_line("a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn a_comma_inside_quotes_does_not_shift_the_columns() {
        assert_eq!(
            split_csv_line(r#"P01,"Fly heading 270, maintain 3000",8000"#),
            vec!["P01", "Fly heading 270, maintain 3000", "8000"]
        );
    }

    #[test]
    fn an_escaped_quote_survives() {
        assert_eq!(split_csv_line(r#"a,"say ""hi""",b"#), vec!["a", r#"say "hi""#, "b"]);
    }

    #[test]
    fn a_trailing_empty_field_is_kept_so_columns_line_up() {
        assert_eq!(split_csv_line("a,b,"), vec!["a", "b", ""]);
    }

    #[test]
    fn columns_are_found_by_name_not_position() {
        let header: Vec<String> = ["flight_id", "heading", "altitude"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let positions = column_positions(&header);
        assert_eq!(positions[0], Some(2), "altitude");
        assert_eq!(positions[3], Some(1), "heading");
        assert_eq!(positions[1], None, "velocity is absent");
    }

    #[test]
    fn airspeed_is_accepted_as_a_name_for_velocity() {
        let header: Vec<String> = ["airspeed"].iter().map(|s| s.to_string()).collect();
        assert_eq!(column_positions(&header)[1], Some(0));
    }

    #[test]
    fn a_payload_carries_both_halves_of_every_value() {
        let header: Vec<String> = ["altitude", "velocity"].iter().map(|s| s.to_string()).collect();
        let row: Vec<String> = ["3000", "120"].iter().map(|s| s.to_string()).collect();
        let payload = payload_for_row(&row, &column_positions(&header));
        let fields: Vec<&str> = payload.split(';').collect();
        assert_eq!(fields.len(), 16, "eight values, both halves: {payload}");
        assert_eq!(fields[0], "3000.000000");
        assert_eq!(fields[1], "3000.000000");
        assert_eq!(fields[2], "120.000000");
        assert_eq!(fields[3], "120.000000");
    }

    #[test]
    fn a_missing_column_is_sent_as_zero_rather_than_shifting_the_rest() {
        let header: Vec<String> = ["altitude"].iter().map(|s| s.to_string()).collect();
        let row: Vec<String> = ["3000"].iter().map(|s| s.to_string()).collect();
        let fields: Vec<String> = payload_for_row(&row, &column_positions(&header))
            .split(';')
            .map(|s| s.to_string())
            .collect();
        assert_eq!(fields[0], "3000.000000");
        assert!(fields[2..].iter().all(|f| f == "0.000000"), "{fields:?}");
    }

    #[test]
    fn an_unparseable_value_is_sent_as_zero_rather_than_stopping_the_replay() {
        let header: Vec<String> = ["altitude"].iter().map(|s| s.to_string()).collect();
        let row: Vec<String> = vec!["".to_string()];
        let payload = payload_for_row(&row, &column_positions(&header));
        assert!(payload.starts_with("0.000000;0.000000"), "{payload}");
    }

    #[test]
    fn a_payload_is_the_shape_the_tcp_thread_expects() {
        // normalize_baton_payload splits on ';' and drops empty fields, then
        // packets_for_fields pairs them. Sixteen non-empty fields is the
        // paired shape, so this must not contain an empty one.
        let header: Vec<String> = ["altitude"].iter().map(|s| s.to_string()).collect();
        let row: Vec<String> = vec!["3000".to_string()];
        let payload = payload_for_row(&row, &column_positions(&header));
        assert!(!payload.contains(";;"), "{payload}");
        assert!(!payload.starts_with(';') && !payload.ends_with(';'), "{payload}");
    }

    #[test]
    fn a_recording_lists_the_flights_in_it() {
        let dir = temp_dir("lists-flights");
        let recording = read_recording(&two_flight_recording(&dir)).unwrap();
        assert_eq!(recording.flights, vec!["P01-F1", "P02-F1"]);
        assert_eq!(recording.label(), "two_flights.csv");
    }

    #[test]
    fn a_recording_with_no_flight_column_lists_no_flights_to_choose_between() {
        let dir = temp_dir("one-flight");
        let path = dir.join("single.csv");
        fs::write(&path, "timestamp,altitude,heading\n1,3000,270\n").unwrap();
        assert!(read_recording(&path).unwrap().flights.is_empty());
        // And it still replays, as the one flight it holds.
        assert_eq!(payloads_for(&path, None).unwrap().len(), 1);
    }

    #[test]
    fn one_flight_is_taken_out_of_a_recording_that_holds_many() {
        let dir = temp_dir("one-of-many");
        let path = two_flight_recording(&dir);
        let payloads = payloads_for(&path, Some("P01-F1")).unwrap();
        assert_eq!(payloads.len(), 3, "P01-F1 has three samples, P02-F1 has one");
        assert_eq!(payloads[0].split(';').count(), 16);
        assert!(payloads[0].starts_with("3000.000000"), "{}", payloads[0]);
    }

    #[test]
    fn a_flight_that_is_not_in_the_recording_is_refused() {
        let dir = temp_dir("no-such-flight");
        let path = two_flight_recording(&dir);
        let error = payloads_for(&path, Some("NOPE")).unwrap_err().to_string();
        assert!(error.contains("No rows for flight NOPE"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_telemetry_is_not_offered() {
        let dir = temp_dir("picker");
        two_flight_recording(&dir);
        // The data folder also holds tiers, labels and feature tables. Someone
        // picking a flight to replay is not expected to know which is which,
        // so a file that cannot be replayed is left out rather than offered
        // and then refused.
        fs::write(dir.join("tiers.csv"), "pilot_id,tier\nP01,2\n").unwrap();
        fs::write(dir.join("notes.txt"), "not a csv at all\n").unwrap();

        let offered = recordings_in(&dir);

        let labels: Vec<String> = offered.iter().map(|r| r.label()).collect();
        assert_eq!(labels, vec!["two_flights.csv"], "{labels:?}");
        assert!(read_recording(&dir.join("tiers.csv")).is_err());
    }

    #[test]
    fn listing_a_folder_that_is_not_there_is_empty_rather_than_a_failure() {
        assert!(recordings_in(Path::new("no/such/folder")).is_empty());
    }

    #[test]
    fn an_empty_file_is_refused_with_its_name() {
        let dir = temp_dir("empty");
        let path = dir.join("empty.csv");
        fs::write(&path, "").unwrap();
        let error = describe_recording(&path).unwrap_err().to_string();
        assert!(error.contains("empty.csv"), "{error}");
    }

    #[test]
    fn a_replay_hands_back_one_sample_per_period() {
        let payloads: Vec<String> = (0..5).map(|i| format!("sample {i}")).collect();
        let mut replay = Replay::new("F1".into(), payloads, 20.0);
        let start = Instant::now();

        // The first call starts the clock and is due one sample.
        assert_eq!(replay.due(start), vec!["sample 0"]);
        // Nothing more until a period has passed.
        assert!(replay.due(start + Duration::from_millis(10)).is_empty());
        assert_eq!(replay.due(start + Duration::from_millis(50)), vec!["sample 1"]);
        assert_eq!(
            replay.due(start + Duration::from_millis(150)),
            vec!["sample 2", "sample 3"],
            "a late tick catches up rather than losing samples"
        );
    }

    #[test]
    fn a_replay_ends_and_stays_ended() {
        let mut replay = Replay::new("F1".into(), vec!["only".to_string()], 20.0);
        let start = Instant::now();
        assert_eq!(replay.due(start), vec!["only"]);
        assert!(replay.finished());
        assert!(replay.due(start + Duration::from_secs(10)).is_empty());
        assert_eq!(replay.sent(), 1);
    }

    #[test]
    fn a_long_stall_does_not_become_a_burst() {
        let payloads: Vec<String> = (0..500).map(|i| i.to_string()).collect();
        let mut replay = Replay::new("F1".into(), payloads, 20.0);
        let start = Instant::now();
        replay.due(start);
        let batch = replay.due(start + Duration::from_secs(60));
        assert_eq!(
            batch.len(),
            MAX_SAMPLES_PER_TICK,
            "a minute of catch-up would be 1200 samples at once"
        );
    }

    #[test]
    fn zero_hz_sends_everything_at_once_for_a_test_that_should_not_wait() {
        let payloads: Vec<String> = (0..7).map(|i| i.to_string()).collect();
        let mut replay = Replay::new("F1".into(), payloads, 0.0);
        assert_eq!(replay.due(Instant::now()).len(), 7);
        assert!(replay.finished());
    }

    #[test]
    fn the_description_says_where_it_has_got_to() {
        let mut replay = Replay::new("P01-F1".into(), vec!["a".into(), "b".into()], 20.0);
        replay.due(Instant::now());
        assert_eq!(replay.describe(), "Replaying P01-F1, sample 1 of 2");
    }
}
