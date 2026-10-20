use opentab_audio::{AudioError, BankError, InstallError, OutputError};
use opentab_command::CommandError;
use opentab_files::FileError;
use opentab_playback::PlaybackError;
use serde::Serialize;

pub const MAX_MESSAGE_CHARS: usize = 1_000;
pub const MAX_ECHO_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolError {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_kind: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

#[derive(Serialize)]
pub struct ErrorEnvelope<'a> {
    pub error: &'a ToolError,
}

impl ToolError {
    pub fn new(kind: &'static str, message: impl AsRef<str>) -> ToolError {
        ToolError {
            kind,
            sub_kind: None,
            message: bounded(message.as_ref(), MAX_MESSAGE_CHARS),
            index: None,
        }
    }

    pub fn usage(message: impl AsRef<str>) -> ToolError {
        ToolError::new("usage", message)
    }

    pub fn invalid_input(message: impl AsRef<str>) -> ToolError {
        ToolError::new("invalid_input", message)
    }

    pub fn internal(message: impl AsRef<str>) -> ToolError {
        ToolError::new("internal", message)
    }

    pub fn invalid_command(index: usize, message: impl AsRef<str>) -> ToolError {
        ToolError {
            index: Some(index),
            ..ToolError::new(
                "invalid_command",
                format!("commands[{index}]: {}", message.as_ref()),
            )
        }
    }

    pub fn command_failed(index: Option<usize>, source: &CommandError) -> ToolError {
        let message = match index {
            Some(index) => format!("commands[{index}]: {source}"),
            None => source.to_string(),
        };
        ToolError {
            sub_kind: command_sub_kind(source),
            index,
            ..ToolError::new("command_failed", message)
        }
    }

    pub fn envelope(&self) -> String {
        serde_json::to_string(&ErrorEnvelope { error: self }).unwrap_or_else(|_| {
            format!(
                "{{\"error\":{{\"kind\":\"{}\",\"message\":\"\"}}}}",
                self.kind
            )
        })
    }
}

fn command_sub_kind(source: &CommandError) -> Option<String> {
    serde_json::to_value(source)
        .ok()?
        .get("kind")?
        .as_str()
        .map(str::to_owned)
}

impl From<FileError> for ToolError {
    fn from(error: FileError) -> ToolError {
        ToolError::new(error.kind(), error.to_string())
    }
}

impl From<AudioError> for ToolError {
    fn from(error: AudioError) -> ToolError {
        ToolError::new("audio", error.to_string())
    }
}

impl From<OutputError> for ToolError {
    fn from(error: OutputError) -> ToolError {
        ToolError::new("audio", error.to_string())
    }
}

impl From<PlaybackError> for ToolError {
    fn from(error: PlaybackError) -> ToolError {
        ToolError::new("audio", error.to_string())
    }
}

impl From<BankError> for ToolError {
    fn from(error: BankError) -> ToolError {
        ToolError::new("sound_bank", error.to_string())
    }
}

impl From<InstallError> for ToolError {
    fn from(error: InstallError) -> ToolError {
        ToolError::new("sound_bank", error.to_string())
    }
}

pub fn bounded(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

pub fn echo(text: &str) -> String {
    format!("{:?}", bounded(text, MAX_ECHO_CHARS))
}

#[cfg(test)]
mod tests {
    use opentab_model::{ElementId, TrackId};

    use super::*;

    #[test]
    fn long_text_is_cut_at_the_limit_on_a_character_boundary() {
        assert_eq!(bounded("abc", 3), "abc");
        assert_eq!(bounded("abcd", 3), "abc…");
        assert_eq!(bounded("ééééé", 2), "éé…");
        assert_eq!(bounded("", 0), "");
        let message = ToolError::invalid_input("x".repeat(5_000)).message;
        assert_eq!(message.chars().count(), MAX_MESSAGE_CHARS + 1);
        assert!(echo(&"y".repeat(500)).chars().count() < MAX_ECHO_CHARS + 4);
    }

    #[test]
    fn command_failures_carry_the_command_error_kind_as_sub_kind() {
        let error = ToolError::command_failed(
            Some(2),
            &CommandError::NotFound {
                what: "beat",
                id: 9,
            },
        );
        assert_eq!(error.kind, "command_failed");
        assert_eq!(error.sub_kind.as_deref(), Some("not_found"));
        assert_eq!(error.index, Some(2));
        assert_eq!(error.message, "commands[2]: beat 9 not found");
        let error = ToolError::command_failed(
            None,
            &CommandError::WrongInstrument {
                track: TrackId(ElementId(4)),
            },
        );
        assert_eq!(error.sub_kind.as_deref(), Some("wrong_instrument"));
        assert_eq!(error.index, None);
    }

    #[test]
    fn the_envelope_matches_the_cli_json_error_shape() {
        let error = ToolError::invalid_command(0, "unknown variant");
        assert_eq!(
            error.envelope(),
            r#"{"error":{"kind":"invalid_command","message":"commands[0]: unknown variant","index":0}}"#
        );
    }
}
