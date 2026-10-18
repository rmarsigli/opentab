use opentab_command::{BeatIds, IdTree};

pub fn human(tree: &IdTree) -> Vec<String> {
    let mut lines = vec!["ids:".to_string()];
    lines.push(format!("  master bars: {}", join(&tree.master_bars)));
    for track in &tree.tracks {
        lines.push(format!("  track {}", track.id));
        for staff in &track.staves {
            lines.push(format!("    staff {}", staff.id));
            for bar in &staff.bars {
                lines.push(format!("      bar {}", bar.id));
                for voice in &bar.voices {
                    lines.push(format!("        voice {}", voice.id));
                    for beat in &voice.beats {
                        lines.push(beat_line(beat));
                    }
                }
            }
        }
    }
    lines
}

fn beat_line(beat: &BeatIds) -> String {
    if beat.notes.is_empty() {
        format!("          beat {}", beat.id)
    } else {
        format!("          beat {}: notes {}", beat.id, join(&beat.notes))
    }
}

fn join(ids: &[u64]) -> String {
    let texts: Vec<String> = ids.iter().map(u64::to_string).collect();
    texts.join(" ")
}
