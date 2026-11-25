//! Command-line options (#276). Without any, the relay starts exactly as it
//! always has and waits for clicks.

use anyhow::{anyhow, bail, Result};

use crate::state::{State, IMOTIONS_ADDR_HINT, MIRROR_ADDR_HINT};

/// What `relay --help` prints, and what follows a rejected command line.
pub(crate) const USAGE: &str = "Usage: relay [--tcp <ip:port>] [--mirror <ip:port>]

Without options the relay waits for you to connect from its window.

Options:
  --tcp <ip:port>     Connect to iMotions (or, with no iMotions, to the Python
                      listener) on start-up, and keep retrying until it
                      connects and again after it drops.
  --mirror <ip:port>  Also copy every packet to the Python listener, with the
                      same retrying. Needs --tcp.
  -h, --help          Show this text.

Addresses are a literal IP and port, for example 127.0.0.1:5001.";

/// Returned by `parse_args` when the command line asks for the usage text. It
/// is not a mistake: the caller prints `USAGE` and exits successfully.
#[derive(Debug)]
pub(crate) struct HelpRequested;

impl std::fmt::Display for HelpRequested {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "help requested")
    }
}

impl std::error::Error for HelpRequested {}

/// Outputs to connect on start-up and keep connected, retrying until each one
/// connects and again after it drops (docs/adr/0005).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct StartupConnections {
    /// `--tcp <ip:port>`: iMotions, or the Python listener when there is no
    /// iMotions.
    pub tcp: Option<String>,
    /// `--mirror <ip:port>`: the Python listener, while `--tcp` goes to
    /// iMotions.
    pub mirror: Option<String>,
}

/// Parse the arguments after the program name.
///
/// `--help` and `-h` give `Err(HelpRequested)`, which the caller recognises
/// with `downcast_ref`.
pub(crate) fn parse_args(args: impl IntoIterator<Item = String>) -> Result<StartupConnections> {
    let mut connections = StartupConnections::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (slot, hint) = match arg.as_str() {
            "--help" | "-h" => return Err(HelpRequested.into()),
            "--tcp" => (&mut connections.tcp, IMOTIONS_ADDR_HINT),
            "--mirror" => (&mut connections.mirror, MIRROR_ADDR_HINT),
            other if other.starts_with('-') => bail!("Unknown option {other}"),
            other => bail!("Unexpected argument {other:?}. Addresses go after --tcp or --mirror"),
        };
        if slot.is_some() {
            bail!("{arg} was given more than once");
        }
        // An address never starts with a dash, so another option here means
        // the address was left out.
        let value = match args.next() {
            Some(value) if !value.trim_start().starts_with('-') => value,
            _ => bail!("{arg} needs an address, for example {hint}"),
        };
        let value = value.trim();
        State::validate_tcp_addr(value, hint).map_err(|e| anyhow!("{arg}: {e}"))?;
        *slot = Some(value.to_string());
    }
    if connections.mirror.is_some() && connections.tcp.is_none() {
        bail!("--mirror needs --tcp too: the mirror copies the TCP stream");
    }
    Ok(connections)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<StartupConnections> {
        parse_args(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn no_options_connect_nothing() {
        assert_eq!(parse(&[]).expect("no options failed"), StartupConnections::default());
    }

    #[test]
    fn tcp_alone_is_the_no_imotions_case() {
        assert_eq!(
            parse(&["--tcp", "127.0.0.1:5001"]).expect("parse failed"),
            StartupConnections {
                tcp: Some("127.0.0.1:5001".into()),
                mirror: None,
            }
        );
    }

    #[test]
    fn tcp_and_mirror_together_is_the_imotions_case() {
        assert_eq!(
            parse(&["--tcp", "192.168.1.20:8089", "--mirror", "127.0.0.1:5001"])
                .expect("parse failed"),
            StartupConnections {
                tcp: Some("192.168.1.20:8089".into()),
                mirror: Some("127.0.0.1:5001".into()),
            }
        );
    }

    #[test]
    fn the_order_of_the_options_does_not_matter() {
        assert_eq!(
            parse(&["--mirror", "127.0.0.1:5001", "--tcp", "192.168.1.20:8089"])
                .expect("parse failed"),
            StartupConnections {
                tcp: Some("192.168.1.20:8089".into()),
                mirror: Some("127.0.0.1:5001".into()),
            }
        );
    }

    #[test]
    fn stray_whitespace_around_an_address_is_trimmed() {
        // A .cmd file that builds the command line from a prompt can leave it.
        assert_eq!(
            parse(&["--tcp", " 127.0.0.1:5001 "]).expect("parse failed").tcp,
            Some("127.0.0.1:5001".into())
        );
    }

    #[test]
    fn a_mirror_without_tcp_is_refused() {
        // The mirror copies the TCP stream, so it has nothing to copy alone.
        let error = parse(&["--mirror", "127.0.0.1:5001"]).expect_err("accepted a lone mirror");
        assert!(error.to_string().contains("--tcp"), "unhelpful error: {error}");
    }

    #[test]
    fn an_unknown_option_is_refused_and_named() {
        let error = parse(&["--port", "5001"]).expect_err("accepted --port");
        assert!(error.to_string().contains("--port"), "unhelpful error: {error}");
    }

    #[test]
    fn an_option_without_its_address_is_refused() {
        let error = parse(&["--tcp"]).expect_err("accepted --tcp with no address");
        assert!(error.to_string().contains("--tcp"), "unhelpful error: {error}");
    }

    #[test]
    fn an_option_followed_by_another_option_is_missing_its_address() {
        let error =
            parse(&["--tcp", "--mirror", "127.0.0.1:5001"]).expect_err("took --mirror as an address");
        assert!(error.to_string().contains("--tcp"), "unhelpful error: {error}");
    }

    #[test]
    fn a_hostname_is_refused_like_in_the_address_box() {
        // The address boxes take a literal IP and port; the options match them.
        assert!(parse(&["--tcp", "localhost:5001"]).is_err());
        assert!(parse(&["--tcp", "127.0.0.1"]).is_err(), "accepted an address with no port");
    }

    #[test]
    fn giving_the_same_option_twice_is_refused() {
        let error = parse(&["--tcp", "127.0.0.1:5001", "--tcp", "127.0.0.1:5002"])
            .expect_err("accepted two --tcp");
        assert!(error.to_string().contains("--tcp"), "unhelpful error: {error}");
    }
}
