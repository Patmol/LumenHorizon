use std::{fmt, io::IsTerminal};

use indicatif::{ProgressBar, ProgressStyle};

pub(crate) struct ProgressTracker {
    bar: ProgressBar,
}

impl ProgressTracker {
    pub(crate) fn set_message(&self, message: impl Into<String>) {
        self.bar.set_message(message.into());
    }

    pub(crate) fn inc(&self, delta: u64) {
        self.bar.inc(delta);
    }

    pub(crate) fn finish(&self, message: impl Into<String>) {
        self.bar.finish_with_message(message.into());
    }
}

pub(crate) fn status(message: fmt::Arguments<'_>) {
    line("info", message);
}

pub(crate) fn success(message: fmt::Arguments<'_>) {
    line("done", message);
}

pub(crate) fn warn(message: fmt::Arguments<'_>) {
    line("warn", message);
}

pub(crate) fn error(message: fmt::Arguments<'_>) {
    line("error", message);
}

pub(crate) fn progress(label: impl Into<String>, total: usize) -> ProgressTracker {
    let bar = if enabled() {
        ProgressBar::new(total as u64)
    } else {
        ProgressBar::hidden()
    };

    bar.set_style(progress_style());
    bar.set_message(label.into());

    ProgressTracker { bar }
}

fn line(level: &str, message: fmt::Arguments<'_>) {
    if !enabled() {
        return;
    }

    let line = format!("[processing-svc] {level}: {message}");
    let bar = ProgressBar::new_spinner();
    bar.println(line);
    bar.finish_and_clear();
}

fn enabled() -> bool {
    match std::env::var("PROCESSING_UI") {
        Ok(value) if is_disabled_value(&value) => false,
        Ok(value) if is_enabled_value(&value) => true,
        _ if cfg!(test) => false,
        _ => std::io::stderr().is_terminal(),
    }
}

fn progress_style() -> ProgressStyle {
    ProgressStyle::with_template("{wide_bar:.cyan/blue} {pos}/{len} {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=> ")
}

fn is_disabled_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    )
}

fn is_enabled_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "yes"
    )
}

#[cfg(test)]
mod tests {
    use super::{is_disabled_value, is_enabled_value};

    #[test]
    fn parses_processing_ui_boolean_values() {
        assert!(is_enabled_value("1"));
        assert!(is_enabled_value("true"));
        assert!(is_disabled_value("0"));
        assert!(is_disabled_value("false"));
    }
}
