use std::fmt;

pub const USAGE: &str =
    "Usage: processing-svc [worker|process-once|process-message <json>|retention-cleanup [--execute]]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Worker,
    ProcessOnce,
    ProcessMessage { message: String },
    RetentionCleanup { execute: bool },
}

impl Command {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::ProcessOnce => "process-once",
            Self::ProcessMessage { .. } => "process-message",
            Self::RetentionCleanup { .. } => "retention-cleanup",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        [] => Ok(CommandRequest::Run(Command::Worker)),
        [arg] if arg == "-h" || arg == "--help" => Ok(CommandRequest::Help),
        [arg] if arg == "worker" => Ok(CommandRequest::Run(Command::Worker)),
        [arg] if arg == "process-once" => Ok(CommandRequest::Run(Command::ProcessOnce)),
        [arg] if arg == "retention-cleanup" => Ok(CommandRequest::Run(Command::RetentionCleanup {
            execute: false,
        })),
        [command, flag] if command == "retention-cleanup" && flag == "--execute" => {
            Ok(CommandRequest::Run(Command::RetentionCleanup {
                execute: true,
            }))
        }
        [command] if command == "process-message" => Err(CommandError::new(
            "process-message requires a JSON message argument",
        )),
        [command, message] if command == "process-message" => {
            Ok(CommandRequest::Run(Command::ProcessMessage {
                message: message.clone(),
            }))
        }
        [arg, ..] if arg == "-h" || arg == "--help" => Err(CommandError::new(
            "help does not accept additional arguments",
        )),
        [command, extra, ..] if command == "worker" || command == "process-once" => {
            Err(CommandError::new(format!(
                "unexpected argument '{extra}' after command '{command}'"
            )))
        }
        [command, extra, ..] if command == "retention-cleanup" => Err(CommandError::new(format!(
            "unexpected argument '{extra}' after retention-cleanup"
        ))),
        [command, extra, ..] if command == "process-message" => Err(CommandError::new(format!(
            "unexpected argument '{extra}' after process-message JSON payload"
        ))),
        [unknown, ..] => Err(CommandError::new(format!("unknown command '{unknown}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, Command, CommandRequest, USAGE};

    #[test]
    fn no_args_default_to_worker() {
        assert_eq!(
            parse_args(Vec::<String>::new()).unwrap(),
            CommandRequest::Run(Command::Worker)
        );
    }

    #[test]
    fn parses_known_commands() {
        assert_eq!(
            parse_args(["worker"]).unwrap(),
            CommandRequest::Run(Command::Worker)
        );
        assert_eq!(
            parse_args(["process-once"]).unwrap(),
            CommandRequest::Run(Command::ProcessOnce)
        );
        assert_eq!(
            parse_args(["process-message", "{\"ingest_id\":\"example\"}"]).unwrap(),
            CommandRequest::Run(Command::ProcessMessage {
                message: "{\"ingest_id\":\"example\"}".to_string(),
            })
        );
        assert_eq!(
            parse_args(["retention-cleanup"]).unwrap(),
            CommandRequest::Run(Command::RetentionCleanup { execute: false })
        );
        assert_eq!(
            parse_args(["retention-cleanup", "--execute"]).unwrap(),
            CommandRequest::Run(Command::RetentionCleanup { execute: true })
        );
    }

    #[test]
    fn parses_help() {
        assert_eq!(parse_args(["--help"]).unwrap(), CommandRequest::Help);
        assert_eq!(parse_args(["-h"]).unwrap(), CommandRequest::Help);
    }

    #[test]
    fn rejects_missing_process_message_payload() {
        let error = parse_args(["process-message"]).unwrap_err().to_string();

        assert!(error.contains("process-message requires a JSON message argument"));
        assert!(error.contains(USAGE));
    }

    #[test]
    fn rejects_unknown_commands() {
        let error = parse_args(["download"]).unwrap_err().to_string();

        assert!(error.contains("unknown command 'download'"));
        assert!(error.contains(USAGE));
    }

    #[test]
    fn rejects_unexpected_retention_cleanup_flag() {
        let error = parse_args(["retention-cleanup", "--dry-run"])
            .unwrap_err()
            .to_string();

        assert!(error.contains("unexpected argument '--dry-run' after retention-cleanup"));
        assert!(error.contains(USAGE));
    }
}
