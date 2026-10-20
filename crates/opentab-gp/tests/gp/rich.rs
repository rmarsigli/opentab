use opentab_model::{BarId, ElementId, NoteId, Score};

use crate::support::read_gpif;

pub const ROOT_MARKER: &str = "root-marker";
pub const INTRO_MARKER: &str = "intro-marker";
pub const SOUND_MARKER: &str = "sound-switch";
pub const SECOND_SOUND: &str = "second-sound";
pub const PITCH_MARKER: &str = "<Number>99</Number>";
pub const ACCENT_MARKER: &str = "<Accent>10</Accent>";
pub const BRUSH_MARKER: &str = "<XProperty id=\"687935490\">";
pub const ORNAMENT_MARKER: &str = "UpperMordent";
pub const UNREFERENCED_MARKER: &str = "LowerMordent";
pub const SIMILE_MARKER: &str = "<SimileMark>Simple</SimileMark>";
pub const MASTER_MARKER: &str = "<XProperty id=\"1124139010\">";
pub const HOPO_DESTINATION: &str = "<Property name=\"HopoDestination\">";
pub const FRAME_OFFSETS: [&str; 3] = ["1000", "2000", "3000"];

pub fn sync(bar: usize, index: usize, occurrence: usize, frame: &str) -> String {
    format!(
        concat!(
            "<Automation><Type>SyncPoint</Type><Linear>false</Linear><Bar>{}</Bar>",
            "<Position>0</Position><Visible>true</Visible><Value><BarIndex>{}</BarIndex>",
            "<BarOccurrence>{}</BarOccurrence><ModifiedTempo>100</ModifiedTempo>",
            "<OriginalTempo>100</OriginalTempo><FrameOffset>{}</FrameOffset></Value></Automation>"
        ),
        bar, index, occurrence, frame
    )
}

fn automation(kind: &str, bar: usize, value: &str) -> String {
    format!(
        "<Automation><Type>{kind}</Type><Linear>false</Linear><Bar>{bar}</Bar><Position>0</Position><Visible>true</Visible><Value>{value}</Value></Automation>"
    )
}

fn master_bar(extra: &str, bars: usize) -> String {
    format!(
        "<MasterBar><Key><AccidentalCount>0</AccidentalCount><Mode>Major</Mode></Key><Time>4/4</Time>{extra}<Bars>{bars}</Bars></MasterBar>"
    )
}

fn note(id: usize, string: u8, fret: u8, before: &str, properties: &str) -> String {
    format!(
        concat!(
            "<Note id=\"{}\">{}<Properties><Property name=\"String\"><String>{}</String></Property>",
            "<Property name=\"Fret\"><Fret>{}</Fret></Property>{}</Properties></Note>"
        ),
        id, before, string, fret, properties
    )
}

const DESTINATION: &str = "<Property name=\"HopoDestination\"><Enable/></Property>";

pub fn rich_gpif() -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<GPIF>\n");
    out.push_str("<GPRevision>13008</GPRevision>\n");
    out.push_str(&format!(
        "<OurRootExtra><Tag>{ROOT_MARKER}</Tag></OurRootExtra>\n"
    ));
    out.push_str("<Score><Title><![CDATA[Rich]]></Title></Score>\n");
    out.push_str("<MasterTrack><Tracks>0</Tracks><Automations>");
    out.push_str(&format!(
        concat!(
            "<Automation><Type>Tempo</Type><Linear>true</Linear><Bar>0</Bar><Position>0</Position>",
            "<Visible>true</Visible><Text><![CDATA[{}]]></Text><Value>100 2</Value></Automation>"
        ),
        INTRO_MARKER
    ));
    out.push_str(
        "<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>1</Bar><Position>0.5</Position><Visible>true</Visible><Value>80 2</Value></Automation>",
    );
    out.push_str(&sync(2, 0, 1, FRAME_OFFSETS[0]));
    out.push_str(&sync(4, 2, 0, FRAME_OFFSETS[1]));
    out.push_str(&sync(5, 3, 0, FRAME_OFFSETS[2]));
    out.push_str("</Automations><RSE><Master><Effect id=\"E1\"><Automations>");
    out.push_str(&automation("DSPParam_00", 3, "0.5"));
    out.push_str("</Automations></Effect></Master></RSE></MasterTrack>\n");
    out.push_str("<Tracks><Track id=\"0\"><Name><![CDATA[Guitar]]></Name><ShortName><![CDATA[gtr]]></ShortName>");
    out.push_str("<InstrumentSet><Type>electricGuitar</Type></InstrumentSet>");
    out.push_str("<RSE><ChannelStrip><Automations>");
    out.push_str(&automation("DSPParam_11", 1, "0.250"));
    out.push_str("</Automations></ChannelStrip></RSE>");
    out.push_str(&format!(
        "<Sounds><Sound><MIDI><Program>25</Program></MIDI></Sound><Sound><Name><![CDATA[{SECOND_SOUND}]]></Name><MIDI><Program>30</Program></MIDI></Sound></Sounds>"
    ));
    out.push_str("<Lyrics dispatched=\"true\"><Line><Text><![CDATA[la la]]></Text><Offset>1</Offset></Line><Line><Text><![CDATA[]]></Text><Offset>0</Offset></Line></Lyrics>");
    out.push_str("<Staves><Staff><Properties><Property name=\"Tuning\"><Pitches>40 45 50 55 59 64</Pitches></Property></Properties></Staff></Staves>");
    out.push_str("<Automations>");
    out.push_str(&automation(
        "Sound",
        2,
        &format!("<![CDATA[{SOUND_MARKER}]]>"),
    ));
    out.push_str("</Automations></Track></Tracks>\n<MasterBars>");
    out.push_str(&master_bar(
        "<Repeat start=\"true\" end=\"false\" count=\"0\"/><XProperties><XProperty id=\"1124139010\"><Int>7</Int></XProperty></XProperties>",
        0,
    ));
    out.push_str(&master_bar(
        "<Repeat start=\"false\" end=\"true\" count=\"2\"/>",
        1,
    ));
    out.push_str(&master_bar(
        "<Section><Letter><![CDATA[A]]></Letter><Text><![CDATA[Verse]]></Text></Section>",
        2,
    ));
    out.push_str(&master_bar("", 3));
    out.push_str("</MasterBars>\n<Bars>");
    out.push_str(&format!(
        "<Bar id=\"0\"><Clef>G2</Clef>{SIMILE_MARKER}<Voices>0 -1 -1 -1</Voices></Bar>"
    ));
    out.push_str("<Bar id=\"1\"><Clef>G2</Clef><Voices>1 -1 -1 -1</Voices></Bar>");
    out.push_str("<Bar id=\"2\"><Clef>G2</Clef><Voices>2 -1 -1 -1</Voices></Bar>");
    out.push_str("<Bar id=\"3\"><Clef>G2</Clef><Voices>-1 -1 -1 -1</Voices></Bar>");
    out.push_str("</Bars>\n<Voices>");
    out.push_str("<Voice id=\"0\"><Beats>0 1</Beats></Voice>");
    out.push_str("<Voice id=\"1\"><Beats>2 3</Beats></Voice>");
    out.push_str("<Voice id=\"2\"><Beats>4</Beats></Voice>");
    out.push_str("</Voices>\n<Beats>");
    out.push_str("<Beat id=\"0\"><Rhythm ref=\"0\"/><Notes>0 1</Notes></Beat>");
    out.push_str(concat!(
        "<Beat id=\"1\"><Rhythm ref=\"0\"/><Notes>2</Notes><Properties><Property name=\"Brush\">",
        "<Direction>Up</Direction></Property></Properties><XProperties>",
        "<XProperty id=\"687935490\"><Float>0.5</Float></XProperty></XProperties></Beat>"
    ));
    out.push_str("<Beat id=\"2\"><Rhythm ref=\"0\"/><Notes>3</Notes></Beat>");
    out.push_str("<Beat id=\"3\"><Rhythm ref=\"0\"/><Notes>4</Notes></Beat>");
    out.push_str("<Beat id=\"4\"><Dynamic>FF</Dynamic><Rhythm ref=\"0\"/><Notes>5</Notes></Beat>");
    out.push_str("</Beats>\n<Notes>");
    out.push_str(&note(
        0,
        0,
        3,
        "",
        "<Property name=\"HopoOrigin\"><Enable/></Property><Property name=\"Midi\"><Number>99</Number></Property>",
    ));
    out.push_str(&note(1, 2, 5, ACCENT_MARKER, ""));
    out.push_str(&note(2, 0, 5, "", DESTINATION));
    out.push_str(&note(
        3,
        1,
        7,
        &format!("<Ornament>{ORNAMENT_MARKER}</Ornament>"),
        "",
    ));
    out.push_str(&note(4, 1, 9, "", DESTINATION));
    out.push_str(&note(5, 0, 0, "", DESTINATION));
    out.push_str(&note(
        6,
        5,
        22,
        &format!("<Ornament>{UNREFERENCED_MARKER}</Ornament>"),
        "",
    ));
    out.push_str("</Notes>\n<Rhythms><Rhythm id=\"0\"><NoteValue>Half</NoteValue></Rhythm></Rhythms>\n</GPIF>\n");
    out
}

pub struct Rich {
    pub score: Score,
    pub master_ids: Vec<ElementId>,
    pub pitched: NoteId,
    pub accented: NoteId,
    pub ornamented: NoteId,
    pub destinations: Vec<NoteId>,
    pub brushed: ElementId,
    pub simile: BarId,
}

pub fn rich() -> Rich {
    let score = read_gpif(&rich_gpif());
    let master_ids = score.master_bars.iter().map(|master| master.id.0).collect();
    let bars = &score.tracks[0].staves[0].bars;
    let first = &bars[0].voices[0].beats;
    let second = &bars[1].voices[0].beats;
    let third = &bars[2].voices[0].beats;
    Rich {
        master_ids,
        pitched: first[0].notes[0].id,
        accented: first[0].notes[1].id,
        ornamented: second[0].notes[0].id,
        destinations: vec![
            first[1].notes[0].id,
            second[1].notes[0].id,
            third[0].notes[0].id,
        ],
        brushed: first[1].id.0,
        simile: bars[0].id,
        score,
    }
}
