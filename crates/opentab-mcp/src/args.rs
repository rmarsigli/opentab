use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Number, Value};

use crate::error::ToolError;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct OpenArgs {
    pub path: String,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct SessionArgs {
    pub session: String,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RenderArgs {
    pub session: String,
    pub page: Option<u32>,
    pub track: Option<TrackArg>,
    pub bars: Option<BarsArg>,
    pub staff: Option<StaffArg>,
    pub scale: Option<ScaleArg>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum TrackArg {
    One(u32),
    Many(Vec<u32>),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum BarsArg {
    One(u32),
    Span(String),
}

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum StaffArg {
    Standard,
    Tab,
    Both,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ScaleArg {
    Number(Number),
    Text(String),
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ApplyArgs {
    pub session: String,
    pub commands: Vec<Value>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ExportArgs {
    pub session: String,
    pub path: String,
}

impl TrackArg {
    pub fn numbers(self) -> Vec<u32> {
        match self {
            TrackArg::One(number) => vec![number],
            TrackArg::Many(numbers) => numbers,
        }
    }
}

impl BarsArg {
    pub fn text(&self) -> String {
        match self {
            BarsArg::One(number) => number.to_string(),
            BarsArg::Span(text) => text.clone(),
        }
    }
}

impl ScaleArg {
    pub fn text(&self) -> String {
        match self {
            ScaleArg::Number(number) => number.to_string(),
            ScaleArg::Text(text) => text.clone(),
        }
    }
}

pub fn parse<T: DeserializeOwned>(
    tool: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<T, ToolError> {
    let value = Value::Object(arguments.unwrap_or_default());
    serde_json::from_value(value)
        .map_err(|error| ToolError::usage(format!("invalid arguments for {tool}: {error}")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn object(value: Value) -> Option<Map<String, Value>> {
        match value {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    #[test]
    fn render_arguments_accept_numbers_or_text_for_tracks_bars_and_scale() {
        let args: RenderArgs = parse(
            "render",
            object(json!({"session": "s", "track": 2, "bars": 3, "scale": 1.5})),
        )
        .unwrap();
        assert_eq!(args.track.unwrap().numbers(), vec![2]);
        assert_eq!(args.bars.unwrap().text(), "3");
        assert_eq!(args.scale.unwrap().text(), "1.5");
        let args: RenderArgs = parse(
            "render",
            object(json!({"session": "s", "track": [1, 3], "bars": "2-4", "scale": "3/2", "staff": "tab", "page": 2})),
        )
        .unwrap();
        assert_eq!(args.track.unwrap().numbers(), vec![1, 3]);
        assert_eq!(args.bars.unwrap().text(), "2-4");
        assert_eq!(args.scale.unwrap().text(), "3/2");
        assert!(matches!(args.staff, Some(StaffArg::Tab)));
        assert_eq!(args.page, Some(2));
    }

    #[test]
    fn missing_unknown_or_mistyped_arguments_are_usage_errors() {
        for value in [
            json!({}),
            json!({"session": 1}),
            json!({"session": "s", "colour": "red"}),
            json!({"session": "s", "page": -1}),
            json!({"session": "s", "page": 4_294_967_296_u64}),
            json!({"session": "s", "staff": "piano"}),
            json!({"session": "s", "track": "one"}),
        ] {
            let error = parse::<RenderArgs>("render", object(value.clone())).unwrap_err();
            assert_eq!(error.kind, "usage", "{value}");
            assert!(error.message.starts_with("invalid arguments for render: "));
        }
        let error = parse::<SessionArgs>("info", None).unwrap_err();
        assert_eq!(error.kind, "usage");
    }
}
