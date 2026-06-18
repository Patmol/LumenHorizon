use std::{fmt, str::FromStr};

use crate::models::ProductCadence;
use uuid::Uuid;

pub const USAGE: &str =
    "Usage: ingest-svc [serve|ingest <daily|monthly>|recover-ingest|replay-rejected <ingest-id>]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Serve,
    Ingest { cadence: ProductCadence },
    RecoverIngest,
    ReplayRejected { ingest_id: Uuid },
}

impl Command {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Serve => "serve",
            Self::Ingest { .. } => "ingest",
            Self::RecoverIngest => "recover-ingest",
            Self::ReplayRejected { .. } => "replay-rejected",
        }
    }

    pub fn ingest_cadence(self) -> Option<ProductCadence> {
        match self {
            Self::Serve | Self::RecoverIngest | Self::ReplayRejected { .. } => None,
            Self::Ingest { cadence } => Some(cadence),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRequest {
    Run(Command),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    message: String,
}

impl CommandError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}\n{}", self.message, USAGE)
    }
}

impl std::error::Error for CommandError {}

pub fn parse_args<I, S>(args: I) -> Result<CommandRequest, CommandError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();

    match args.as_slice() {
        [] => Ok(CommandRequest::Run(Command::Serve)),
        [arg] if arg == "-h" || arg == "--help" => Ok(CommandRequest::Help),
        [command] if command == "recover-ingest" => Ok(CommandRequest::Run(Command::RecoverIngest)),
        [arg] => parse_command(arg).map(CommandRequest::Run),
        [command, cadence] if command == "ingest" => parse_ingest_cadence(cadence)
            .map(|cadence| CommandRequest::Run(Command::Ingest { cadence })),
        [command, ingest_id] if command == "replay-rejected" => parse_ingest_id(ingest_id)
            .map(|ingest_id| CommandRequest::Run(Command::ReplayRejected { ingest_id })),
        [arg, ..] if arg == "-h" || arg == "--help" => Err(CommandError::new(
            "help does not accept additional arguments",
        )),
        [command, extra, ..] => Err(CommandError::new(format!(
            "unexpected argument '{extra}' after command '{command}'"
        ))),
    }
}

fn parse_command(value: &str) -> Result<Command, CommandError> {
    match value {
        "serve" => Ok(Command::Serve),
        "ingest" => Err(CommandError::new("ingest requires a cadence argument")),
        "replay-rejected" => Err(CommandError::new("replay-rejected requires an ingest id")),
        unknown => Err(CommandError::new(format!("unknown command '{unknown}'"))),
    }
}

fn parse_ingest_cadence(value: &str) -> Result<ProductCadence, CommandError> {
    ProductCadence::parse(value)
        .ok_or_else(|| CommandError::new(format!("unknown ingest cadence '{value}'")))
}

fn parse_ingest_id(value: &str) -> Result<Uuid, CommandError> {
    Uuid::from_str(value).map_err(|_| CommandError::new(format!("invalid ingest id '{value}'")))
}

#[cfg(test)]
mod tests {
    use crate::models::ProductCadence;

    use super::{parse_args, Command, CommandRequest};

    #[test]
    fn no_args_default_to_serve() {
        assert_eq!(
            parse_args(Vec::<String>::new()).unwrap(),
            CommandRequest::Run(Command::Serve)
        );
    }

    #[test]
    fn parses_known_commands() {
        assert_eq!(
            parse_args(["ingest", "daily"]).unwrap(),
            CommandRequest::Run(Command::Ingest {
                cadence: ProductCadence::Daily
            })
        );
        assert_eq!(
            parse_args(["ingest", "monthly"]).unwrap(),
            CommandRequest::Run(Command::Ingest {
                cadence: ProductCadence::Monthly
            })
        );
        assert_eq!(
            parse_args(["recover-ingest"]).unwrap(),
            CommandRequest::Run(Command::RecoverIngest)
        );
        assert_eq!(
            parse_args(["replay-rejected", "11111111-1111-4111-8111-111111111111"]).unwrap(),
            CommandRequest::Run(Command::ReplayRejected {
                ingest_id: "11111111-1111-4111-8111-111111111111".parse().unwrap()
            })
        );
    }

    #[test]
    fn rejects_ingest_without_cadence() {
        let error = parse_args(["ingest"]).unwrap_err().to_string();

        assert!(error.contains("ingest requires a cadence argument"));
        assert!(error.contains("recover-ingest"));
    }

    #[test]
    fn parses_help() {
        assert_eq!(parse_args(["--help"]).unwrap(), CommandRequest::Help);
        assert_eq!(parse_args(["-h"]).unwrap(), CommandRequest::Help);
    }

    #[test]
    fn rejects_unknown_commands() {
        let error = parse_args(["download"]).unwrap_err().to_string();

        assert!(error.contains("unknown command 'download'"));
        assert!(error.contains("recover-ingest"));
    }

    #[test]
    fn rejects_unknown_ingest_cadence() {
        let error = parse_args(["ingest", "weekly"]).unwrap_err().to_string();

        assert!(error.contains("unknown ingest cadence 'weekly'"));
        assert!(error.contains("recover-ingest"));
    }

    #[test]
    fn rejects_replay_without_valid_id() {
        let error = parse_args(["replay-rejected", "not-a-uuid"])
            .unwrap_err()
            .to_string();

        assert!(error.contains("invalid ingest id 'not-a-uuid'"));
        assert!(error.contains("replay-rejected <ingest-id>"));
    }

    #[test]
    fn rejects_migrate_command() {
        let error = parse_args(["migrate"]).unwrap_err().to_string();

        assert!(error.contains("unknown command 'migrate'"));
        assert!(error.contains("recover-ingest"));
    }
}
