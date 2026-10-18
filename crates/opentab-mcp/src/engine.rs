use std::path::Path;

use opentab_command::{Command, Info};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::args::{self, ApplyArgs, ExportArgs, OpenArgs, RenderArgs, SessionArgs};
use crate::error::ToolError;
use crate::files;
use crate::render::{RenderedPage, render};
use crate::session::{Session, Sessions};
use crate::tools::ToolName;

pub enum Reply {
    Json(Value),
    Image { png: Vec<u8>, report: Value },
}

pub enum CallError {
    UnknownTool,
    Tool(ToolError),
}

#[derive(Serialize)]
pub struct TrackEntry {
    pub number: usize,
    pub name: String,
    pub kind: &'static str,
}

#[derive(Serialize)]
pub struct Opened {
    pub session: String,
    pub title: String,
    pub tracks: Vec<TrackEntry>,
    pub bars: usize,
}

#[derive(Serialize)]
pub struct SessionInfo {
    pub session: String,
    pub path: String,
    pub can_undo: bool,
    pub can_redo: bool,
    #[serde(flatten)]
    pub info: Info,
}

#[derive(Serialize)]
pub struct Applied {
    pub applied: usize,
    pub allocated: Vec<u64>,
}

#[derive(Serialize)]
pub struct Undone {
    pub undone: bool,
    pub commands: usize,
}

#[derive(Serialize)]
pub struct Redone {
    pub redone: bool,
    pub commands: usize,
}

#[derive(Serialize)]
pub struct Exported {
    pub exported: String,
    pub format: &'static str,
}

#[derive(Serialize)]
pub struct Closed {
    pub closed: String,
}

#[derive(Default)]
pub struct Engine {
    sessions: Sessions,
}

impl Engine {
    pub fn call(
        &mut self,
        tool: &str,
        arguments: Option<Map<String, Value>>,
    ) -> Result<Reply, CallError> {
        let tool = ToolName::parse(tool).ok_or(CallError::UnknownTool)?;
        self.dispatch(tool, arguments).map_err(CallError::Tool)
    }

    fn dispatch(
        &mut self,
        tool: ToolName,
        arguments: Option<Map<String, Value>>,
    ) -> Result<Reply, ToolError> {
        let name = tool.name();
        match tool {
            ToolName::Open => self.open(args::parse(name, arguments)?),
            ToolName::Info => self.info(args::parse(name, arguments)?),
            ToolName::Render => self.render(args::parse(name, arguments)?),
            ToolName::Apply => self.apply(args::parse(name, arguments)?),
            ToolName::Undo => self.undo(args::parse(name, arguments)?),
            ToolName::Redo => self.redo(args::parse(name, arguments)?),
            ToolName::Export => self.export(args::parse(name, arguments)?),
            ToolName::Close => self.close(args::parse(name, arguments)?),
        }
    }

    fn open(&mut self, args: OpenArgs) -> Result<Reply, ToolError> {
        let score = files::read_score(Path::new(&args.path))?;
        let info = Info::of(&score);
        let tracks = info
            .tracks
            .into_iter()
            .enumerate()
            .map(|(index, track)| TrackEntry {
                number: index + 1,
                name: track.name,
                kind: track.kind,
            })
            .collect();
        let session = self.sessions.insert(Session::new(args.path, score))?;
        json(&Opened {
            session,
            title: info.title,
            tracks,
            bars: info.master_bars,
        })
    }

    fn info(&mut self, args: SessionArgs) -> Result<Reply, ToolError> {
        let session = self.sessions.get(&args.session)?;
        json(&SessionInfo {
            path: session.path.clone(),
            can_undo: session.can_undo(),
            can_redo: session.can_redo(),
            info: Info::of(&session.score),
            session: args.session,
        })
    }

    fn render(&mut self, args: RenderArgs) -> Result<Reply, ToolError> {
        let session = self.sessions.get(&args.session)?;
        let picture = render(&session.score, args)?;
        Ok(Reply::Image {
            png: picture.png,
            report: value::<RenderedPage>(&picture.report)?,
        })
    }

    fn apply(&mut self, args: ApplyArgs) -> Result<Reply, ToolError> {
        let session = self.sessions.get(&args.session)?;
        let mut commands = Vec::with_capacity(args.commands.len());
        for (index, raw) in args.commands.into_iter().enumerate() {
            let command: Command = serde_json::from_value(raw)
                .map_err(|error| ToolError::invalid_command(index, error.to_string()))?;
            commands.push(command);
        }
        let applied = commands.len();
        let allocated = session
            .apply(commands)
            .map_err(|failure| ToolError::command_failed(Some(failure.index), &failure.error))?;
        json(&Applied { applied, allocated })
    }

    fn undo(&mut self, args: SessionArgs) -> Result<Reply, ToolError> {
        let step = self.sessions.get(&args.session)?.undo()?;
        json(&Undone {
            undone: step.done,
            commands: step.commands,
        })
    }

    fn redo(&mut self, args: SessionArgs) -> Result<Reply, ToolError> {
        let step = self.sessions.get(&args.session)?.redo()?;
        json(&Redone {
            redone: step.done,
            commands: step.commands,
        })
    }

    fn export(&mut self, args: ExportArgs) -> Result<Reply, ToolError> {
        let session = self.sessions.get(&args.session)?;
        let format = files::write_score(Path::new(&args.path), &session.score)?;
        json(&Exported {
            exported: args.path,
            format: format.name(),
        })
    }

    fn close(&mut self, args: SessionArgs) -> Result<Reply, ToolError> {
        self.sessions.remove(&args.session)?;
        json(&Closed {
            closed: args.session,
        })
    }
}

fn value<T: Serialize>(reply: &T) -> Result<Value, ToolError> {
    serde_json::to_value(reply).map_err(|error| ToolError::internal(error.to_string()))
}

fn json<T: Serialize>(reply: &T) -> Result<Reply, ToolError> {
    value(reply).map(Reply::Json)
}
