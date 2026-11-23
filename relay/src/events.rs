//! Read the JSON events a listener emits under `--json`.
//!
//! The contract is defined in `inference/pilot_inference/events.py` and pinned
//! from that side by `inference/tests/test_events.py`. Scraping the console
//! text instead would mean a Rust regex against Python print statements, and
//! the person rewording one of those is not going to run these tests.
//!
//! Anything that is not a known event is handed back as `None` rather than
//! being an error. uv writes its own progress to stderr, a traceback is not
//! JSON, and a newer listener may emit a kind this build has never heard of.
//! All three belong in the raw output pane, not in a crash.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The version `events.py` emits. A listener reporting anything else is still
/// read, because every field is optional, but it is worth saying once.
pub(crate) const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct Feature {
    #[serde(default)]
    pub(crate) std: f64,
    #[serde(default)]
    pub(crate) abs_bias: f64,
}

/// What `status_payload` in live_groups.py reports.
///
/// Every field has a default, so a listener that gains or loses one keeps
/// working here instead of failing to parse the whole line.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub(crate) struct Status {
    #[serde(default)]
    pub(crate) state: String,
    #[serde(default)]
    pub(crate) given: u32,
    #[serde(default)]
    pub(crate) total: u32,
    #[serde(default)]
    pub(crate) settling: Vec<String>,
    #[serde(default)]
    pub(crate) settling_all: bool,
    #[serde(default)]
    pub(crate) scored_seconds: f64,
    #[serde(default)]
    pub(crate) min_scored: f64,
    #[serde(default)]
    pub(crate) provisional: bool,
    #[serde(default)]
    pub(crate) group: Option<i64>,
    #[serde(default)]
    pub(crate) features: BTreeMap<String, Feature>,
}

impl Status {
    /// The headline the Instructor reads mid-flight.
    pub(crate) fn headline(&self) -> String {
        match self.state.as_str() {
            "waiting" => "Waiting for the first instruction".to_string(),
            "nothing_scored" => {
                format!("Instruction {}/{}, nothing scored yet", self.given, self.total)
            }
            _ => {
                let group = match (self.provisional, self.group) {
                    (false, Some(group)) => format!("group {group}"),
                    _ => format!(
                        "provisional, {:.0}s scored of {:.0}s needed",
                        self.scored_seconds, self.min_scored
                    ),
                };
                format!(
                    "Instruction {}/{}, scored {:.0}s, {}",
                    self.given, self.total, self.scored_seconds, group
                )
            }
        }
    }

    /// Which factors are not being scored yet, in a few words.
    pub(crate) fn settling_text(&self) -> Option<String> {
        if self.settling.is_empty() {
            return None;
        }
        if self.settling_all {
            return Some("settling, nothing scored".to_string());
        }
        Some(format!("settling: {}", self.settling.join(", ")))
    }

    /// One line per flight dynamics factor, as the console shows them.
    pub(crate) fn feature_lines(&self) -> Vec<String> {
        self.features
            .iter()
            .map(|(metric, value)| {
                format!("{metric} std {:.1}  |bias| {:.1}", value.std, value.abs_bias)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ListenerEvent {
    Fitting {
        k: u32,
        cohort_pilots: u32,
    },
    Cohort {
        agreement: f64,
        description: String,
    },
    Started {
        pilot_id: String,
        flight_id: String,
        port: u16,
        csv: String,
        total: u32,
        next: Option<String>,
        unnamed_pilot: bool,
    },
    Status(Status),
    Instruction {
        given: u32,
        total: u32,
        text: String,
        next: Option<String>,
    },
    Stopped {
        scored_seconds: f64,
        given: u32,
    },
    Error {
        message: String,
    },
}

#[derive(Deserialize)]
struct Fitting {
    #[serde(default)]
    k: u32,
    #[serde(default)]
    cohort_pilots: u32,
}

#[derive(Deserialize)]
struct Cohort {
    #[serde(default)]
    agreement: f64,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct Started {
    #[serde(default)]
    pilot_id: String,
    #[serde(default)]
    flight_id: String,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    csv: String,
    #[serde(default)]
    total: u32,
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    unnamed_pilot: bool,
}

#[derive(Deserialize)]
struct InstructionGiven {
    #[serde(default)]
    given: u32,
    #[serde(default)]
    total: u32,
    #[serde(default)]
    text: String,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Deserialize)]
struct Stopped {
    #[serde(default)]
    scored_seconds: f64,
    #[serde(default)]
    given: u32,
}

#[derive(Deserialize)]
struct ErrorEvent {
    #[serde(default)]
    message: String,
}

/// Parse one line of listener output.
///
/// `None` means the line is not an event this build understands, which is the
/// normal case for uv's own chatter and for a Python traceback.
pub(crate) fn parse_event(line: &str) -> Option<ListenerEvent> {
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let kind = value.get("event")?.as_str()?;

    // Deserialising from the same object twice is cheap here, and it keeps
    // every variant's fields optional without a hand-written visitor.
    let event = match kind {
        "fitting" => {
            let parsed: Fitting = serde_json::from_value(value.clone()).ok()?;
            ListenerEvent::Fitting {
                k: parsed.k,
                cohort_pilots: parsed.cohort_pilots,
            }
        }
        "cohort" => {
            let parsed: Cohort = serde_json::from_value(value.clone()).ok()?;
            ListenerEvent::Cohort {
                agreement: parsed.agreement,
                description: parsed.description,
            }
        }
        "started" => {
            let parsed: Started = serde_json::from_value(value.clone()).ok()?;
            ListenerEvent::Started {
                pilot_id: parsed.pilot_id,
                flight_id: parsed.flight_id,
                port: parsed.port,
                csv: parsed.csv,
                total: parsed.total,
                next: parsed.next,
                unnamed_pilot: parsed.unnamed_pilot,
            }
        }
        "status" => ListenerEvent::Status(serde_json::from_value(value.clone()).ok()?),
        "instruction" => {
            let parsed: InstructionGiven = serde_json::from_value(value.clone()).ok()?;
            ListenerEvent::Instruction {
                given: parsed.given,
                total: parsed.total,
                text: parsed.text,
                next: parsed.next,
            }
        }
        "stopped" => {
            let parsed: Stopped = serde_json::from_value(value.clone()).ok()?;
            ListenerEvent::Stopped {
                scored_seconds: parsed.scored_seconds,
                given: parsed.given,
            }
        }
        "error" => {
            let parsed: ErrorEvent = serde_json::from_value(value.clone()).ok()?;
            ListenerEvent::Error {
                message: parsed.message,
            }
        }
        _ => return None,
    };
    Some(event)
}

/// The protocol version a line declares, if it is an event at all.
pub(crate) fn protocol_version(line: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    value.get("v")?.as_u64().map(|v| v as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines captured from a real `--json` run, so these tests fail if the
    /// Python side changes shape.
    const FITTING: &str = r#"{"event": "fitting", "v": 1, "k": 3, "cohort_pilots": 15, "seed": 0}"#;
    const COHORT: &str = r#"{"event": "cohort", "v": 1, "agreement": 0.7920792079207921, "description": "  group 0 (6 pilots): altitude std 39.8"}"#;
    const STARTED: &str = r#"{"event": "started", "v": 1, "host": "127.0.0.1", "port": 5066, "pilot_id": "P004", "flight_id": "JSONTEST", "csv": "D:\\x\\live.csv", "instructions_csv": "D:\\x\\i.csv", "total": 4, "next": "Fly heading 270", "unnamed_pilot": false}"#;
    const INSTRUCTION: &str = r#"{"event": "instruction", "v": 1, "given": 1, "total": 4, "text": "Fly heading 270", "settling": {"altitude": 30, "heading": 30}, "next": "Turn right heading 360"}"#;
    const STOPPED: &str = r#"{"event": "stopped", "v": 1, "scored_seconds": 12.0, "given": 1}"#;

    fn status(json: &str) -> Status {
        match parse_event(json).expect("a status event") {
            ListenerEvent::Status(status) => status,
            other => panic!("expected a status, got {other:?}"),
        }
    }

    #[test]
    fn a_fitting_event_is_read() {
        assert_eq!(
            parse_event(FITTING),
            Some(ListenerEvent::Fitting {
                k: 3,
                cohort_pilots: 15
            })
        );
    }

    #[test]
    fn a_cohort_event_keeps_the_agreement_and_the_table() {
        match parse_event(COHORT).unwrap() {
            ListenerEvent::Cohort {
                agreement,
                description,
            } => {
                assert!((agreement - 0.79).abs() < 0.01);
                assert!(description.contains("group 0"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_started_event_carries_the_ids_and_the_first_instruction() {
        match parse_event(STARTED).unwrap() {
            ListenerEvent::Started {
                pilot_id,
                flight_id,
                total,
                next,
                port,
                ..
            } => {
                assert_eq!(pilot_id, "P004");
                assert_eq!(flight_id, "JSONTEST");
                assert_eq!(total, 4);
                assert_eq!(port, 5066);
                assert_eq!(next.as_deref(), Some("Fly heading 270"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_instruction_event_carries_what_is_next() {
        match parse_event(INSTRUCTION).unwrap() {
            ListenerEvent::Instruction {
                given, total, next, ..
            } => {
                assert_eq!((given, total), (1, 4));
                assert_eq!(next.as_deref(), Some("Turn right heading 360"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_stopped_event_is_read() {
        assert_eq!(
            parse_event(STOPPED),
            Some(ListenerEvent::Stopped {
                scored_seconds: 12.0,
                given: 1
            })
        );
    }

    #[test]
    fn uv_chatter_is_not_an_event() {
        assert_eq!(parse_event("Installed 1 package in 10ms"), None);
        assert_eq!(parse_event(""), None);
        assert_eq!(parse_event("Traceback (most recent call last):"), None);
    }

    #[test]
    fn a_kind_this_build_does_not_know_is_ignored_rather_than_fatal() {
        assert_eq!(parse_event(r#"{"event": "from_the_future", "v": 2}"#), None);
    }

    #[test]
    fn a_truncated_line_is_ignored() {
        assert_eq!(parse_event(r#"{"event": "status", "given""#), None);
    }

    #[test]
    fn the_protocol_version_is_readable() {
        assert_eq!(protocol_version(STARTED), Some(PROTOCOL_VERSION));
        assert_eq!(protocol_version("not json"), None);
    }

    #[test]
    fn a_waiting_status_says_so() {
        let status = status(r#"{"event":"status","v":1,"state":"waiting","given":0,"total":4}"#);
        assert_eq!(status.headline(), "Waiting for the first instruction");
        assert_eq!(status.settling_text(), None);
    }

    #[test]
    fn a_settling_status_names_the_factors() {
        let status = status(
            r#"{"event":"status","v":1,"state":"nothing_scored","given":1,"total":4,
                "settling":["altitude","heading"],"settling_all":false}"#,
        );
        assert_eq!(status.headline(), "Instruction 1/4, nothing scored yet");
        assert_eq!(status.settling_text().unwrap(), "settling: altitude, heading");
    }

    #[test]
    fn everything_settling_is_said_once_rather_than_listed() {
        let status = status(
            r#"{"event":"status","v":1,"state":"nothing_scored","given":1,"total":4,
                "settling":["altitude","heading","vertical_speed","airspeed"],"settling_all":true}"#,
        );
        assert_eq!(status.settling_text().unwrap(), "settling, nothing scored");
    }

    #[test]
    fn a_provisional_status_says_how_much_more_is_needed() {
        let status = status(
            r#"{"event":"status","v":1,"state":"scoring","given":2,"total":4,
                "scored_seconds":18.0,"min_scored":60.0,"provisional":true,"group":null}"#,
        );
        assert_eq!(
            status.headline(),
            "Instruction 2/4, scored 18s, provisional, 18s scored of 60s needed"
        );
    }

    #[test]
    fn a_named_group_is_the_headline() {
        let status = status(
            r#"{"event":"status","v":1,"state":"scoring","given":3,"total":4,
                "scored_seconds":92.0,"min_scored":60.0,"provisional":false,"group":1}"#,
        );
        assert_eq!(status.headline(), "Instruction 3/4, scored 92s, group 1");
    }

    #[test]
    fn the_features_are_one_line_each() {
        let status = status(
            r#"{"event":"status","v":1,"state":"scoring","given":1,"total":1,
                "features":{"altitude":{"std":12.34,"abs_bias":4.5},
                            "heading":{"std":1.2,"abs_bias":0.34}}}"#,
        );
        let lines = status.feature_lines();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l == "altitude std 12.3  |bias| 4.5"));
        assert!(lines.iter().any(|l| l == "heading std 1.2  |bias| 0.3"));
    }

    #[test]
    fn a_status_missing_fields_still_parses() {
        // A listener that drops a field must not take the window down.
        let status = status(r#"{"event":"status","v":1,"state":"scoring"}"#);
        assert_eq!(status.given, 0);
        assert!(status.features.is_empty());
    }
}
