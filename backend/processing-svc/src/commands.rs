use std::fmt;

use chrono::NaiveDate;

pub const USAGE: &str = "Usage: processing-svc [worker|process-once|process-message <json>|retention-cleanup [--execute]|publish-mosaic <product> [<YYYY-MM-DD>|latest] [--public-latest [--allow-incomplete-public-latest]]]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Worker,
    ProcessOnce,
    ProcessMessage {
        message: String,
    },
    RetentionCleanup {
        execute: bool,
    },
    PublishMosaic {
        product: String,
        dataset_date: Option<NaiveDate>,
        promote_public_latest: bool,
        allow_incomplete_public_latest: bool,
    },
}

impl Command {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::ProcessOnce => "process-once",
            Self::ProcessMessage { .. } => "process-message",
            Self::RetentionCleanup { .. } => "retention-cleanup",
            Self::PublishMosaic { .. } => "publish-mosaic",
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
        [command] if command == "publish-mosaic" => {
            Err(CommandError::new("publish-mosaic requires a product"))
        }
        [command, product, rest @ ..] if command == "publish-mosaic" => {
            parse_publish_mosaic_args(product, rest)
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

fn parse_publish_mosaic_args(
    product: &str,
    args: &[String],
) -> Result<CommandRequest, CommandError> {
    let (dataset_date, remaining_args) = match args.split_first() {
        Some((value, remaining_args)) if value == "latest" => (None, remaining_args),
        Some((value, _)) if value.starts_with("--") => (None, args),
        Some((value, remaining_args)) => (Some(parse_dataset_date(value)?), remaining_args),
        None => (None, args),
    };

    let (promote_public_latest, allow_incomplete_public_latest) = match remaining_args {
        [] => (false, false),
        [flag] if flag == "--public-latest" => (true, false),
        [public_flag, allow_flag]
            if public_flag == "--public-latest"
                && allow_flag == "--allow-incomplete-public-latest" =>
        {
            (true, true)
        }
        [flag] if flag == "--allow-incomplete-public-latest" => {
            return Err(CommandError::new(
                "--allow-incomplete-public-latest requires --public-latest",
            ));
        }
        [extra, ..] => {
            return Err(CommandError::new(format!(
                "unexpected argument '{extra}' after publish-mosaic arguments"
            )));
        }
    };

    Ok(CommandRequest::Run(Command::PublishMosaic {
        product: product.to_owned(),
        dataset_date,
        promote_public_latest,
        allow_incomplete_public_latest,
    }))
}

fn parse_dataset_date(value: &str) -> Result<NaiveDate, CommandError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| CommandError::new("publish-mosaic dataset date must use YYYY-MM-DD"))
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
        assert_eq!(
            parse_args(["publish-mosaic", "VNP46A2", "2026-05-21"]).unwrap(),
            CommandRequest::Run(Command::PublishMosaic {
                product: "VNP46A2".to_owned(),
                dataset_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap()),
                promote_public_latest: false,
                allow_incomplete_public_latest: false,
            })
        );
        assert_eq!(
            parse_args(["publish-mosaic", "VNP46A2"]).unwrap(),
            CommandRequest::Run(Command::PublishMosaic {
                product: "VNP46A2".to_owned(),
                dataset_date: None,
                promote_public_latest: false,
                allow_incomplete_public_latest: false,
            })
        );
        assert_eq!(
            parse_args(["publish-mosaic", "VNP46A2", "latest", "--public-latest",]).unwrap(),
            CommandRequest::Run(Command::PublishMosaic {
                product: "VNP46A2".to_owned(),
                dataset_date: None,
                promote_public_latest: true,
                allow_incomplete_public_latest: false,
            })
        );
        assert_eq!(
            parse_args(["publish-mosaic", "VNP46A2", "--public-latest",]).unwrap(),
            CommandRequest::Run(Command::PublishMosaic {
                product: "VNP46A2".to_owned(),
                dataset_date: None,
                promote_public_latest: true,
                allow_incomplete_public_latest: false,
            })
        );
        assert_eq!(
            parse_args(["publish-mosaic", "VNP46A2", "2026-05-21", "--public-latest",]).unwrap(),
            CommandRequest::Run(Command::PublishMosaic {
                product: "VNP46A2".to_owned(),
                dataset_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap()),
                promote_public_latest: true,
                allow_incomplete_public_latest: false,
            })
        );
        assert_eq!(
            parse_args([
                "publish-mosaic",
                "VNP46A2",
                "2026-05-21",
                "--public-latest",
                "--allow-incomplete-public-latest",
            ])
            .unwrap(),
            CommandRequest::Run(Command::PublishMosaic {
                product: "VNP46A2".to_owned(),
                dataset_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 21).unwrap()),
                promote_public_latest: true,
                allow_incomplete_public_latest: true,
            })
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
    fn rejects_invalid_publish_mosaic_arguments() {
        let missing_product = parse_args(["publish-mosaic"]).unwrap_err().to_string();
        assert!(missing_product.contains("publish-mosaic requires a product"));

        let invalid_date = parse_args(["publish-mosaic", "VNP46A2", "20260521"])
            .unwrap_err()
            .to_string();
        assert!(invalid_date.contains("dataset date must use YYYY-MM-DD"));

        let unexpected_flag = parse_args(["publish-mosaic", "VNP46A2", "2026-05-21", "--force"])
            .unwrap_err()
            .to_string();
        assert!(unexpected_flag.contains("unexpected argument '--force'"));

        let allow_without_public_latest = parse_args([
            "publish-mosaic",
            "VNP46A2",
            "2026-05-21",
            "--allow-incomplete-public-latest",
        ])
        .unwrap_err()
        .to_string();
        assert!(allow_without_public_latest.contains("requires --public-latest"));
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
