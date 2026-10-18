use opentab_model::Score;

use crate::dom::Element;
use crate::error::GpifProblem;
use crate::location::Location;
use crate::opaque::ParentPath;
use crate::read::context::{Context, Fault, Handled, Owner, Scope};
use crate::read::master::{read_master_bars, read_master_track};
use crate::read::pools::{POOLS, Pools, index_pool};
use crate::read::track::read_tracks;

struct Deferred<'e> {
    master_track: Option<(&'e Element, Location)>,
    master_bars: Option<(&'e Element, Location)>,
    pools: Vec<(&'e Element, Location)>,
}

pub(crate) fn read(root: &Element) -> Result<Score, Fault> {
    let location = Location::root();
    if root.name != "GPIF" {
        return Err(Fault::at(
            &location,
            GpifProblem::UnexpectedRoot {
                found: root.name.clone(),
            },
        ));
    }
    let mut ctx = Context::new();
    let document = Scope::new(Owner::Document, ParentPath::owner(), location);
    ctx.keep_attributes(&document, root, &[]);
    let mut deferred = Deferred {
        master_track: None,
        master_bars: None,
        pools: Vec::new(),
    };
    ctx.visit(&document, root, |ctx, visit| match visit.key.as_str() {
        "Score" => {
            read_meta(ctx, &document.enter(visit), visit.element)?;
            Ok(Handled::Mapped(&[]))
        }
        "MasterTrack" => {
            deferred.master_track = Some((visit.element, visit.location.clone()));
            Ok(Handled::Mapped(&[]))
        }
        "MasterBars" => {
            deferred.master_bars = Some((visit.element, visit.location.clone()));
            Ok(Handled::Mapped(&[]))
        }
        name if POOLS.iter().any(|(pool, _, _)| *pool == name) => {
            deferred.pools.push((visit.element, visit.location.clone()));
            Ok(Handled::Mapped(&[]))
        }
        _ => Ok(Handled::Unknown),
    })?;
    let mut pools = Pools::new();
    for (element, location) in &deferred.pools {
        for (pool, item, kind) in POOLS {
            if element.name == pool {
                index_pool(&mut ctx, &mut pools, element, location, item, kind)?;
            }
        }
    }
    let frames = match deferred.master_bars {
        Some((element, location)) => read_master_bars(&mut ctx, element, &location)?,
        None => Vec::new(),
    };
    let order = match deferred.master_track {
        Some((element, location)) => read_master_track(&mut ctx, element, &location)?,
        None => None,
    };
    read_tracks(&mut ctx, &mut pools, order, &frames)?;
    pools.check_referenced()?;
    Ok(ctx.score)
}

fn read_meta(ctx: &mut Context, scope: &Scope, element: &Element) -> Result<(), Fault> {
    ctx.visit(scope, element, |ctx, visit| {
        let meta = &mut ctx.score.meta;
        let field = match visit.key.as_str() {
            "Title" => &mut meta.title,
            "SubTitle" => &mut meta.subtitle,
            "Artist" => &mut meta.artist,
            "Album" => &mut meta.album,
            "Words" => &mut meta.words,
            "Music" => &mut meta.music,
            "Copyright" => &mut meta.copyright,
            "Tabber" => &mut meta.tab_author,
            "Instructions" => &mut meta.instructions,
            "Notices" => &mut meta.notices,
            _ => return Ok(Handled::Unknown),
        };
        *field = visit.text();
        Ok(Handled::Mapped(&[]))
    })
}
