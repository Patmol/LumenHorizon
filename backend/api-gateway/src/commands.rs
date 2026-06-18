use std::fmt;

pub const USAGE: &str = "Usage: api-gateway [serve]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRequest {
    Run,
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
        [] => Ok(CommandRequest::Run),
        [arg] if arg == "-h" || arg == "--help" => Ok(CommandRequest::Help),
        [arg] if arg == "serve" => Ok(CommandRequest::Run),
        [arg, ..] if arg == "-h" || arg == "--help" => Err(CommandError::new(
            "help does not accept additional arguments",
        )),
        [unknown] => Err(CommandError::new(format!("unknown command '{unknown}'"))),
        [command, extra, ..] => Err(CommandError::new(format!(
            "unexpected argument '{extra}' after command '{command}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, CommandRequest};

    #[test]
    fn no_args_default_to_serve() {
        assert_eq!(
            parse_args(Vec::<String>::new()).unwrap(),
            CommandRequest::Run
        );
    }

    #[test]
    fn parses_serve() {
        assert_eq!(parse_args(["serve"]).unwrap(), CommandRequest::Run);
    }

    #[test]
    fn parses_help() {
        assert_eq!(parse_args(["--help"]).unwrap(), CommandRequest::Help);
        assert_eq!(parse_args(["-h"]).unwrap(), CommandRequest::Help);
    }

    #[test]
    fn rejects_unknown_commands() {
        let error = parse_args(["proxy"]).unwrap_err().to_string();

        assert!(error.contains("unknown command 'proxy'"));
        assert!(error.contains("Usage: api-gateway [serve]"));
    }
}
