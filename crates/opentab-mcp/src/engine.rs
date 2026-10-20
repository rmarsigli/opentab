use std::path::Path;

use opentab_audio::{Transport, TransportCommand, TransportHandle};
use opentab_command::{Command, Info};
use opentab_files::{Overwrite, format_of, read_score, require_regular_file, write_score};
use opentab_model::Score;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::args::{
    self, ApplyArgs, AssignArgs, ExportArgs, ExportMidiArgs, InstallArgs, NoArgs, OpenArgs,
    RenderArgs, SessionArgs, SoundArgs,
};
use crate::audio;
use crate::banks;
use crate::error::ToolError;
use crate::midi::export_midi;
use crate::player::{Active, Launch, Player, StatusWire, Worker};
use crate::render::{RenderedPage, render};
use crate::report::Details;
use crate::session::{Session, Sessions};
use crate::sound::prepare;
use crate::tools::ToolName;

pub enum Reply {
    Json(Value),
    Image { png: Vec<u8>, report: Value },
    Audio { wav: Vec<u8>, report: Value },
}

pub enum CallError {
    UnknownTool,
    Tool(ToolError),
}

pub struct RenderJob {
    pub score: Score,
    pub args: SoundArgs,
}

pub enum Job {
    Render(Box<RenderJob>),
    Install(InstallArgs),
}

pub enum Outcome {
    Done(Reply),
    Deferred(Job),
}

impl Job {
    pub fn run(self) -> Result<Reply, ToolError> {
        match self {
            Job::Render(job) => {
                let RenderJob { score, args } = *job;
                let wav = audio::render(&score, args)?;
                Ok(Reply::Audio {
                    wav: wav.bytes,
                    report: value(&wav.report)?,
                })
            }
            Job::Install(args) => json(&banks::install(args)?),
        }
    }
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

#[derive(Serialize)]
pub struct Playing {
    #[serde(flatten)]
    pub status: StatusWire,
    #[serde(flatten)]
    pub details: Details,
}

#[derive(Default)]
pub struct Engine {
    sessions: Sessions,
    player: Player,
}

fn deliver(transport: &mut Transport, handle: &mut TransportHandle, command: TransportCommand) {
    while handle.send(command).is_err() {
        transport.process(&mut [], &mut []);
    }
}

impl Engine {
    pub fn call(
        &mut self,
        tool: &str,
        arguments: Option<Map<String, Value>>,
    ) -> Result<Outcome, CallError> {
        let tool = ToolName::parse(tool).ok_or(CallError::UnknownTool)?;
        self.dispatch(tool, arguments).map_err(CallError::Tool)
    }

    fn dispatch(
        &mut self,
        tool: ToolName,
        arguments: Option<Map<String, Value>>,
    ) -> Result<Outcome, ToolError> {
        let name = tool.name();
        let reply = match tool {
            ToolName::Open => self.open(args::parse(name, arguments)?),
            ToolName::Info => self.info(args::parse(name, arguments)?),
            ToolName::Render => self.render(args::parse(name, arguments)?),
            ToolName::Apply => self.apply(args::parse(name, arguments)?),
            ToolName::Undo => self.undo(args::parse(name, arguments)?),
            ToolName::Redo => self.redo(args::parse(name, arguments)?),
            ToolName::Export => self.export(args::parse(name, arguments)?),
            ToolName::Close => self.close(args::parse(name, arguments)?),
            ToolName::ExportMidi => self.export_midi(args::parse(name, arguments)?),
            ToolName::RenderAudio => return self.render_audio(args::parse(name, arguments)?),
            ToolName::Play => self.play(args::parse(name, arguments)?),
            ToolName::Stop => self.stop(args::parse(name, arguments)?),
            ToolName::Status => self.status(args::parse(name, arguments)?),
            ToolName::Soundbanks => self.soundbanks(args::parse(name, arguments)?),
            ToolName::ImportSoundbank => json(&banks::import(args::parse(name, arguments)?)?),
            ToolName::InstallSoundbank => {
                return Ok(Outcome::Deferred(Job::Install(args::parse(
                    name, arguments,
                )?)));
            }
            ToolName::AssignSoundbank => {
                json(&banks::assign(args::parse::<AssignArgs>(name, arguments)?)?)
            }
        };
        reply.map(Outcome::Done)
    }

    fn open(&mut self, args: OpenArgs) -> Result<Reply, ToolError> {
        let path = Path::new(&args.path);
        format_of(path)?;
        require_regular_file(path)?;
        let score = read_score(path)?;
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
        let path = Path::new(&args.path);
        let format = format_of(path)?;
        write_score(path, &session.score, Overwrite::Allowed)?;
        json(&Exported {
            exported: args.path,
            format: format.name(),
        })
    }

    fn export_midi(&mut self, args: ExportMidiArgs) -> Result<Reply, ToolError> {
        let session = self.sessions.get(&args.session)?;
        json(&export_midi(&session.score, args)?)
    }

    fn render_audio(&mut self, args: SoundArgs) -> Result<Outcome, ToolError> {
        let score = self.sessions.get(&args.session)?.score.clone();
        Ok(Outcome::Deferred(Job::Render(Box::new(RenderJob {
            score,
            args,
        }))))
    }

    fn play(&mut self, args: SoundArgs) -> Result<Reply, ToolError> {
        let session = args.session.clone();
        let score = self.sessions.get(&session)?.score.clone();
        self.player.stop();
        let worker = Worker::open()?;
        let prepared = prepare(&score, args, worker.rate())?;
        let details = prepared.details(&prepared.backends);
        let commands = prepared.commands();
        let (mut transport, mut handle) = prepared.session.split();
        for command in commands {
            deliver(&mut transport, &mut handle, command);
        }
        transport.process(&mut [], &mut []);
        let status = handle.status();
        worker.launch(Launch { transport, handle })?;
        self.player.active = Some(Active {
            worker,
            session: session.clone(),
        });
        json(&Playing {
            status: StatusWire::of(&session, &status),
            details,
        })
    }

    fn soundbanks(&mut self, _args: NoArgs) -> Result<Reply, ToolError> {
        json(&banks::list()?)
    }

    fn stop(&mut self, _args: NoArgs) -> Result<Reply, ToolError> {
        json(&self.player.stop())
    }

    fn status(&mut self, _args: NoArgs) -> Result<Reply, ToolError> {
        json(&self.player.status()?)
    }

    fn close(&mut self, args: SessionArgs) -> Result<Reply, ToolError> {
        self.sessions.remove(&args.session)?;
        self.player.close(&args.session);
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
