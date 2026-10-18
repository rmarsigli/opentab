# The Guitar Pro 7/8 `.gp` format, as OpenTab needs it

This is a clean-room description. It is derived only from two sources: files that Guitar Pro 8.1.5 (build 31, GPIF revision 13008) wrote itself, and experiments in which we handed GP 8 files we built and watched whether it opened them, what it showed, and what it wrote back on "Save As". No vendor documentation, source code or third-party parser was consulted. Snippets below are schematic: element names are real, every value is a placeholder.

Words used in this document:

- **GP** means Guitar Pro 8.1.5.
- **Accepted** means GP opened the file with no error notice and did not crash.
- **Re-save** means GP opened a file and saved it with "Save As" into a new path.
- **Experiment files** named in brackets, such as [min-all], are kept locally in the oracle machine's experiments folder (`C:\gf-oracle\corpus\experiments\`). They are not part of the repository; a bracketed name identifies the experiment files whose names start with it.

## 1. Container

A `.gp` file is a ZIP archive. GP writes these entries, in this order:

| Entry | Content | Role |
|---|---|---|
| `Content/` | directory entry | — |
| `Content/BinaryStylesheet` | binary | engraving style |
| `Content/LayoutConfiguration` | small binary | page and system layout state |
| `Content/PartConfiguration` | small binary | which tracks appear in each part (score view) and with which staves |
| `Content/Preferences.json` | JSON | per-document playback and view preferences |
| `Content/ScoreViews/` and `Content/ScoreViews/<n>.gpsv` | binary, one per part | per-part view state |
| `Content/Stylesheets/` and `Content/Stylesheets/*.gpss` | XML-like text | per-part style overrides |
| `Content/Assets/<uuid>.<ext>` | audio | only in scores with a backing track (5 of 27 examples, `.ogg`) |
| `Content/score.gpif` | XML | the music; everything OpenTab models lives here |
| `VERSION` | ASCII text | container version, always `7.0` |
| `meta.json` | JSON | `{}` or a small object (for example a flag saying the score has audio) |

How GP writes the archive:

- Directory entries are stored uncompressed and empty. All other entries are deflated, including `score.gpif`.
- Local headers set the data-descriptor flag, so CRC and sizes follow the data.
- The DOS date and time fields are zero. Most tools display that as 1980-00-00, which is not a valid calendar date. There are no extra fields and no comments.
- The "made by" field says Unix.

GP also accepts other ways of writing the archive. We rebuilt an example with stored entries, with no data descriptors, with timestamps of 1980-01-01, without the directory entries and with the entries in a different order. GP opened every variant [b-rebuilt, b-rebuilt-stored, e2-order-stored].

### 1.1 Minimal container

We found the minimal set by deleting one entry at a time from an example, then by building archives from nothing:

| Removed | Result |
|---|---|
| `VERSION` | GP refuses the file with an error notice |
| `Content/PartConfiguration` | GP **crashes** after showing the document (seen twice) [d3-drop, c-gpif-version] |
| any other single entry | accepted |

The minimal container has three entries, in any order:

- `Content/score.gpif`;
- `VERSION`, containing `7.0`;
- `Content/PartConfiguration`.

An archive with exactly these three entries was accepted. That held when `PartConfiguration` was a copy from an example, an 8-byte "zero parts" record, or **zero bytes long** [e1-min3, e3-pc0, e4-pcempty]. The empty form carries no vendor-authored content, so a writer that generates a container from scratch should use it. On re-save, GP writes every other entry again with its defaults. It also writes a real `PartConfiguration`.

`PartConfiguration`, as far as we decoded it, is laid out as follows. We never need to write a non-empty one.

- A big-endian u32 part count.
- For each part: one flag byte, a big-endian u32 track count, and one byte per track. Bit 0 of that byte is standard notation and bit 1 is tablature.
- A trailing big-endian u32.

## 2. `score.gpif` as an XML document

- UTF-8 with an XML declaration.
- One element per line in most places. The pitch sub-elements of a note are an exception: GP writes them on one line.
- GP writes exactly one comment, immediately inside `MasterBars` (a reminder that order matters). GP does not need it.
- Children of the root, in GP's order: `GPVersion`, `GPRevision`, `Encoding`, `Score`, `MasterTrack`, `BackingTrack`?, `Tracks`, `MasterBars`, `Bars`, `Voices`, `Beats`, `Notes`, `Rhythms`, `Assets`?, `ScoreViews`.

### 2.1 Text values: CDATA is mandatory

GP writes free-text values as a single CDATA section:

- score metadata;
- track name and short name;
- the tuning name;
- sound names and labels;
- section letter and text;
- beat free text, beat lyrics and track lyrics;
- the value of a sound automation;
- the chord reference of a beat;
- asset paths.

**GP reads these values only from the first CDATA section.** We tested this by changing the title of an example to escaped plain text. GP showed an empty title and saved it empty [u3-plain-title]. A value split across two CDATA sections came back as only the first chunk. Chord references written as plain text were dropped, together with the chord diagrams they pointed to.

Consequences for a writer:

- Every text field listed above must be written as exactly one `<![CDATA[...]]>`.
- A value that contains `]]>` cannot be written in a form GP reads back whole.
- Opaque payloads must keep their raw XML, CDATA included. A parsed text value is not enough.

Numbers, enums and id lists are plain element text.

## 3. Identity and references

GPIF stores the score as flat pools that point at each other by id:

```
MasterBars/MasterBar   (no id; position = bar index)
  Bars        "b b b"          one bar id per staff, in track order
Bars/Bar id=b
  Voices      "v v v v"        always 4 slots; -1 = empty slot
Voices/Voice id=v
  Beats       "e e e"          beat ids in time order
Beats/Beat id=e
  Rhythm ref=r                 duration
  Notes       "n n"            absent on rests
Notes/Note id=n
Rhythms/Rhythm id=r
```

Every other kind of element also lives in its own flat pool with `id` attributes: tracks, bars, voices, beats, notes and rhythms. In all 27 examples, the ids of each pool are `0..n-1` in document order. Every id is referenced, and no reference dangles.

Id lists are space-separated decimals. `MasterTrack/Tracks` lists the track ids in display order.

The `Bars` list of a master bar has one entry per staff of the score. The staves are counted in track order and then in staff order. Every track in the corpus has exactly one staff, so the list has one entry per track.

### 3.1 Sharing (rhythm, beat and note deduplication)

Elements of three kinds are shared, and elements of three are not:

| Element | Shared? |
|---|---|
| Rhythms | Always shared. No two rhythms in a file have equal content. 5 057 beats use 184 rhythms in total. |
| Beats | Shared. One beat element can appear in many voices and several times in the same voice: 15 224 beat references point at 5 057 beat elements, and 999 voices repeat a beat. Shared beats include beats with notes (2 200) and rests (122). |
| Notes | Shared across beats: 6 690 note references point at 3 619 notes. This includes 143 notes that carry a tie. |
| Bars, voices, tracks | Never shared. |

GP rebuilds the sharing whenever it saves. We wrote an example with every reference made unique: 352 beats and 424 notes, instead of 133 of each. GP accepted it. On re-save it produced exactly the same `score.gpif` as for the shared version [u2-blues-unshared].

So a reader must treat a shared beat or note as a value. It copies the element once for each place that references it, and each copy is a separate model element. A writer may share or not; GP accepts either.

## 4. Durations (`Rhythms`)

```
<Rhythm id="r">
  <NoteValue>NAME</NoteValue>
  <AugmentationDot count="D"/>        optional
  <PrimaryTuplet num="N" den="M"/>    optional
</Rhythm>
```

The `NoteValue` names are `Whole`, `Half`, `Quarter`, `Eighth`, `16th`, `32nd`, `64th` and `128th`. `128th` did not occur in the corpus, so we tested it: a score of 128th notes was accepted, re-saved with `128th` unchanged, and exported as MusicXML `128th` notes [n128].

`AugmentationDot/@count` is the number of dots. The corpus has 1 and 2.

`PrimaryTuplet num="N" den="M"` means N notes in the time of M. Its length is the plain length multiplied by M/N. GP's MusicXML export of `num=3 den=2` gives `actual-notes 3` and `normal-notes 2`. The corpus has 3:2 and 4:3. A `SecondaryTuplet` (a nested tuplet) never occurred.

**Grace notes do not take time.** A beat with a `GraceNotes` child (`BeforeBeat` or `OnBeat`) still has a rhythm, but the bar is filled without it. Of the 52 voices that contain grace beats, every one is exactly full when the grace beats are left out, and overfull when they are counted. A voice whose beats are all grace beats is treated like an empty voice.

## 5. Master track and tempo

```
<MasterTrack>
  <Anacrusis/>                      optional: the first bar is a pickup (it may be shorter than the time signature)
  <Tracks>t t t</Tracks>
  <Automations>
    <Automation>
      <Type>Tempo</Type>
      <Linear>false</Linear>        true = ramp to the next tempo point
      <Bar>B</Bar>                  master bar index
      <Position>P</Position>        0..1, fraction of the bar's length
      <Visible>true</Visible>
      <Text><![CDATA[...]]></Text>  optional label
      <Value>BPM UNIT</Value>
    </Automation>
  </Automations>
  <RSE>…</RSE>                      mixer and master effects
</MasterTrack>
```

`BPM` may have a fraction, for example a value with three decimals. `UNIT` is the note the metronome mark counts. We found the codes by exporting each one to MusicXML:

| Code | Unit |
|---|---|
| 1 | eighth |
| 2 | quarter (37 of the 39 tempo automations in the corpus; one uses 1 and one uses 3) |
| 3 | dotted quarter |
| 4 | half |
| 5 | dotted half |
| 0 | treated as quarter, and rewritten to 2 on re-save |

MusicXML playback tempo is converted to quarter notes. For example, 80 with unit 1 plays at 40 quarter notes per minute.

`Position` is written with at most six significant digits, for example `0.333333`. It is not an exact rational.

With no tempo automation at all, GP plays the score at 120: the MusicXML export of a score without one carries a quarter = 120 metronome mark [min-all].

The other master automation type seen is `SyncPoint`. It aligns bars with a backing-track audio file and appears only in scores that have assets. Its `Value` holds sub-elements (bar index, occurrence, frame offset, tempi).

## 6. Master bars

```
<MasterBar>
  <Key><AccidentalCount>K</AccidentalCount><Mode>Major|Minor</Mode><TransposeAs>Sharps|Flats</TransposeAs></Key>
  <Time>N/D</Time>
  <Fermatas>…</Fermatas>                         optional
  <TripletFeel>Triplet8th</TripletFeel>          optional
  <Repeat start="true|false" end="true|false" count="C"/>   optional
  <Section><Letter><![CDATA[..]]></Letter><Text><![CDATA[..]]></Text></Section>   optional
  <AlternateEndings>E E</AlternateEndings>       optional
  <Directions><Target>..</Target><Jump>..</Jump></Directions>   optional
  <DoubleBar/>                                   optional
  <Bars>b b</Bars>
  <XProperties>…</XProperties>                   optional
</MasterBar>
```

- `AccidentalCount` runs from -7 to 7. Negative counts are flats.
- `Repeat` combines a start flag, an end flag and a play count (`count`). The count is 2 on repeat ends and usually 0 on starts. Three starts carry 2.
- `AlternateEndings` lists the ending numbers that cover this bar. The corpus only has single values (1 or 2).
- `Section` is a rehearsal mark: a letter and a text. Either may be empty. The letter is the mark shown beside the text.
- `DoubleBar` marks a double bar line at the end of the bar.
- `Directions` holds segno and coda targets and jumps such as `DaCoda`, `DaSegno` and `DaSegnoAlCoda`.

## 7. Tracks and staves

```
<Track id="t">
  <Name><![CDATA[..]]></Name><ShortName><![CDATA[..]]></ShortName>
  <Color>R G B</Color> … layout, playing style …
  <InstrumentSet>
    <Name>..</Name><Type>TYPE</Type><LineCount>N</LineCount>
    <Elements>…articulation table…</Elements>
  </InstrumentSet>
  <Transpose><Chromatic>C</Chromatic><Octave>O</Octave></Transpose>
  <RSE>…</RSE>
  <Sounds><Sound>…<MIDI><LSB>.</LSB><MSB>.</MSB><Program>P</Program></MIDI>…</Sound></Sounds>
  <MidiConnection>…channels…</MidiConnection>
  <Lyrics dispatched="true"><Line><Text><![CDATA[..]]></Text><Offset>N</Offset></Line> ×5</Lyrics>
  <Staves><Staff><Properties>
      <Property name="CapoFret"><Fret>F</Fret></Property>
      <Property name="FretCount"><Number>N</Number></Property>
      <Property name="PartialCapoFret">…</Property>
      <Property name="PartialCapoStringFlags">…</Property>
      <Property name="Tuning"><Pitches>p p p p p p</Pitches>…</Property>
      <Property name="ChordCollection">…</Property>
      <Property name="DiagramCollection"><Items><Item id="c" name="NAME">…</Item></Items></Property>
      …
      <Name><![CDATA[tuning name]]></Name>
  </Properties></Staff></Staves>
  <Automations>…Sound changes…</Automations>
</Track>
```

- **InstrumentSet/Type** names the instrument family. The corpus has these types:
  - stringed: `electricGuitar`, `steelGuitar`, `nylonGuitar`, `electricBass`, `acousticBass`, `violin`;
  - other pitched: `acousticPiano`, `saxophone`, `trumpet`, `electricOrgan`, `voice`;
  - percussion: `drumKit`.

  A drum kit lists its articulations (54 elements in each drum track of the corpus). A pitched instrument lists a single "pitched" element.
- **Every non-drum track is stored as a string instrument.** Piano, saxophone, trumpet, organ and voice tracks all carry a six-string tuning, and their notes carry string and fret. Drum tracks carry a dummy tuning of six zeros.
- **Tuning** `Pitches` are MIDI numbers, **lowest string first**. The corpus has these tunings:
  - guitar: standard, drop D, and a half step down;
  - bass: 4-string, standard and others;
  - violin: G D A E.
- **CapoFret** is the capo fret. Two tracks have a non-zero capo.
- **Transpose** is the written-to-sounding offset used for standard notation. Guitars and basses carry `Octave -1`, because they are written an octave above concert pitch. Transposing instruments and detuned guitars also carry a `Chromatic` offset. 49 of 64 tracks have a non-zero transpose.
- **Sounds/Sound/MIDI/Program** is the General MIDI program. Two tracks have two sounds, and `Sound` automations on the track switch between them over time.
- The tuning name (`Staff/Properties/Name`) is sometimes cleared by GP on re-save; see §13.

## 8. Bars, voices, beats

```
<Bar id="b"><Clef>G2|F4|Neutral</Clef><SimileMark>…</SimileMark>?<Voices>v -1 -1 -1</Voices></Bar>
<Voice id="v"><Beats>e e e</Beats></Voice>
<Beat id="e">
  <GraceNotes>BeforeBeat|OnBeat</GraceNotes>?   <Dynamic>PPP..FFF</Dynamic>
  <Rhythm ref="r"/>   stem orientations   effects…
  <Chord><![CDATA[c]]></Chord>?   <FreeText><![CDATA[..]]></FreeText>?
  <Notes>n n</Notes>?   <Properties>…</Properties>   <Lyrics>…</Lyrics>?   <XProperties>…</XProperties>?
</Beat>
```

- **Clef** is stored per bar: `G2` is treble, `F4` is bass, and `Neutral` is percussion. 618 bars (bass and percussion staves) are not treble.
- **Voices** always has four slots, and the slot number matters: stem direction and rest placement depend on it. A used slot can follow an empty one: 6 bars in 3 files use only the second voice. 59 bars have no voice at all. Slots in a bar are strictly increasing in the order the voices are kept, and the numbers run from 0 to 3.
- **Dynamic** is on every beat and takes `PPP`, `PP`, `P`, `MP`, `MF`, `F`, `FF` or `FFF`. The default is `MF`, which GP also fills in when the element is missing. 1 629 beats in 16 files have another value. GP's MusicXML export shows a dynamic mark at each change.
- **Chord** is the id of an item in the same staff's `DiagramCollection`. The item's `name` attribute is the chord name shown above the staff.
- Beat child order, as GP writes it:
  1. `GraceNotes`, `Dynamic`, `Rhythm`;
  2. the stem orientation elements;
  3. `Whammy`, `Variation`, `Wah`, `FreeText`, `Golpe`, `Slashed`, `Hairpin`, `Legato`, `Arpeggio`, `Chord`;
  4. `Notes`, `Properties`, `Lyrics`, `XProperties`.

Beat effects the corpus shows. Elements are written `<Name>` and named properties `Property Name`:

| Effect | Where it is stored |
|---|---|
| Brush stroke | `Property Brush` holding `Direction` `Up` or `Down`, plus two beat `XProperty` entries. Id 687935489 holds an `Int`: the brush duration in ticks of **480 per quarter note**. GP's dialog offers 30 (64th) to 480 (quarter) on a continuous slider, and the default is 480 [fx-brush-d-default, fx-brush-home, fx-brush-half-default]. The corpus holds values off the note grid, such as 45, 57 and 147. Id 687935490 holds a `Float` from 0 to 1: the dialog's start-time slider, where 0 is "falls on time" and 1, the default, is "starts on time" [fx-brush-start]. |
| Pick direction | `Property PickStroke`, holding a `Direction`. |
| Slap and pop | `Property Slapped` and `Property Popped`, each with an `Enable` child. |
| Rasgueado and golpe | `Property Rasgueado` and `<Golpe>`. |
| Wah | `<Wah>`. |
| Bar chord marking | `BarreFret` and `BarreString`. |
| Whammy bar | `<Whammy>`, whose attributes give a four-point curve. |
| Arpeggio | `<Arpeggio>`. |
| Hairpins | `<Hairpin>`, `Crescendo` or `Decrescendo`. |
| Slurs across beats | `<Legato origin destination>`. |
| Free text | `<FreeText>`. |

Effects that never occurred in the corpus. We applied each one through GP's Effects menu to a score we built, saved it, and compared the result with a plain re-save [fx-plain]:

| Effect | Where it is stored |
|---|---|
| Tremolo picking | Beat element `<Tremolo>`, holding a fraction: `1/2` for eighth notes, `1/4` for sixteenths, `1/8` for thirty-seconds. These are the only three values GP's menu offers. The value does not depend on the beat's own duration [fx-trem8, fx-trem16, fx-trem32, fx-trem8-half]. |
| Fade in | Beat element `<Fadding>FadeIn</Fadding>`. The element name is spelled this way by GP [fx-fadein]. |
| Tapping | **Not a beat effect in GPIF.** It is a note property: `Property Tapped` with an `Enable` child, on the note under the cursor. Left-hand tapping is `Property LeftHandTapped` [fx-tap, fx-lhtap]. |

## 9. Notes

```
<Note id="n">
  <Vibrato>Slight|Wide</Vibrato>? <AntiAccent>Normal</AntiAccent>? <LeftFingering>F</LeftFingering>?
  <LetRing/>? <Tie origin="true|false" destination="true|false"/>? <Accent>BITS</Accent>?
  <RightFingering>F</RightFingering>? <InstrumentArticulation>A</InstrumentArticulation>
  <Properties>
    <Property name="ConcertPitch"><Pitch><Step>S</Step><Accidental>A</Accidental><Octave>O</Octave></Pitch></Property>
    <Property name="Fret"><Fret>F</Fret></Property>
    <Property name="Midi"><Number>M</Number></Property>
    <Property name="String"><String>S</String></Property>
    <Property name="TransposedPitch"><Pitch>…</Pitch></Property>
    …effect properties…
  </Properties>
</Note>
```

- **String** is 0-based and counts from the **lowest** string. With 6 strings, GPIF string 1 is the A string; GP's MusicXML export calls it string 5.
- **Fret** is the fret number. For drum notes, `InstrumentArticulation` is an index into the track's articulation table. For pitched notes it is 0.
- **Midi** is the sounding MIDI number. **ConcertPitch** and **TransposedPitch** spell that note as step, accidental (`#`, `b` or empty) and octave. Here octave is the MIDI number divided by 12, so MIDI 48 is octave 4. TransposedPitch applies the track's `Transpose`.
- These three properties are derived data, but they must be **written consistently with string, fret, tuning and transpose.** Two experiments show this:
  - A bare score, whose track has no `Transpose`, had notes with only string and fret. GP filled in all three [min-all].
  - Real examples behaved differently. Removing ConcertPitch and TransposedPitch, or all three properties, made GP **silently lose the notes**: 133 notes collapsed to 6 on re-save. Removing only `Midi` changed three notes [t3]. Removing only the stem orientations changed 11 bytes of the re-saved file [t1]. The losses are in [t4], [t5-ex1] and [t6].
- **Accent** is a bit set. We read the meaning of each bit from GP's MusicXML export:

  | Bit | Meaning |
  |---|---|
  | 1 | staccato |
  | 2 | staccatissimo |
  | 4 | heavy accent (marcato) |
  | 8 | accent |
  | 16 | tenuto |

  The corpus has 1, 4, 8, 1+8 and 1+4.
- **AntiAccent `Normal`** is a ghost note; MusicXML shows it as a parenthesised notehead.
- **Fingering** values are `Open`, `P`, `I`, `M`, `A` and `C`: open string, thumb, index, middle, ring and little finger. GP's export maps the left hand to T, 1, 2, 3, 4 and the right hand to p, i, m, a, c. `LeftFingering` takes all six values and `RightFingering` takes the last five.
- **Tie**: the `origin` attribute starts a tie and `destination` ends one. The same note can do both.

Effect properties on notes:

| Property | Content | Meaning |
|---|---|---|
| `HopoOrigin` | `Enable` | Starts a hammer-on or pull-off. |
| `HopoDestination` | `Enable` | Ends one. GP does **not** recompute it on re-save, so it is real data. |
| `Muted` | `Enable` | Dead note. |
| `PalmMuted` | `Enable` | Palm mute. |
| `Slide` | `Flags` | Bit set: 1 shift slide, 2 legato slide, 4 slide out downwards, 8 slide out upwards, 16 slide in from below, 32 slide in from above. Bit 64 is accepted but has no visible export. The corpus only has single bits. |
| `Harmonic`, `HarmonicType`, `HarmonicFret` | `Enable`, `HType`, `HFret` | `HType` is `Natural`, `Artificial`, `Pinch`, `Tap`, `Semi` or `Feedback`. `HFret` is a float such as `12.000000` or `7.000000`. |
| `Bended` + `Bend{Origin,Middle,Destination}{Offset,Value}` | floats | See below. |
| `Tapped`, `LeftHandTapped` | `Enable` | Tapping with the right or the left hand. Tapping is a note property, not a beat property. |
| `ShowStringNumber` | `Enable` | Display only. |

**Bends** have a fixed shape of four points:

1. origin: an offset and a value;
2. middle 1: offset 1 and the shared middle value;
3. middle 2: offset 2 and the shared middle value;
4. destination: an offset and a value.

Offsets run from 0 to 100 across the note. **A value of 100 is a full tone**: GP exports 100 as `bend-alter 2` and 150 as `bend-alter 3`. So 25 units are one quarter tone, and the unit is one hundredth of a semitone (a cent). Observed values are 0, 12, 25, 37.5, 50, 75, 100 and 150. All observed offsets are whole numbers.

## 10. Mapping GPIF ↔ OpenTab model

Notation used in this table:

- "id" means GPIF ids are read as references and re-assigned in document order when writing. Model ids come from the model's allocator.
- "→ opaque" means the GPIF data stays as an opaque payload of the owning element, with enough position information to restore its place among its siblings.
- A modelled value that has no GPIF encoding is never guessed: the writer refuses it with an "unsupported" error.

### Score and metadata

| Model field | GPIF source | Notes |
|---|---|---|
| `Score.meta.title` | `Score/Title` | CDATA |
| `Score.meta.subtitle` | `Score/SubTitle` | CDATA |
| `Score.meta.artist` | `Score/Artist` | CDATA |
| `Score.meta.album` | `Score/Album` | CDATA |
| `Score.meta.words` | `Score/Words` | CDATA |
| `Score.meta.music` | `Score/Music` | CDATA |
| `Score.meta.copyright` | `Score/Copyright` | CDATA |
| `Score.meta.tab_author` | `Score/Tabber` | CDATA |
| `Score.meta.instructions` | `Score/Instructions` | CDATA |
| `Score.meta.notices` | `Score/Notices` | CDATA |
| `Score.master_bars` | `MasterBars/MasterBar`, in order | — |
| `Score.tracks` | `Tracks/Track`, in `MasterTrack/Tracks` order | — |
| `Score.anacrusis` | `MasterTrack/Anacrusis` present | — |
| `Score.extensions` | opaque GPIF keyed by owner id | — |
| `Score.document_extensions` | opaque root-level GPIF: `GPVersion`, `GPRevision`, `Encoding`, `BackingTrack`, `Assets`, `ScoreViews` and similar | Not tied to any element. |
| `Score.container` | every archive entry except `Content/score.gpif`, in archive order, with its exact bytes | Stored as base64 text in the JSON form. |
| `Score.ids` | — | The model allocator; not stored in GPIF. |

### Master bars

| Model field | GPIF source | Notes |
|---|---|---|
| `MasterBar.id` | — | MasterBar has no GPIF id; it is identified by its position. |
| `time_signature.numerator`, `.denominator` | `Time` `N/D` | — |
| `key_signature.accidentals` | `Key/AccidentalCount` | `TransposeAs` → opaque. |
| `key_signature.minor` | `Key/Mode` = `Minor` | — |
| `repeat_start` | `Repeat/@start` | — |
| `repeat_end` | `Repeat/@end` = true → `Some(@count)` | A count on a start-only repeat → opaque. |
| `alternate_endings` (bitmask) | `AlternateEndings`: ending k sets bit k-1 | — |
| `marker.text` | `Section/Text` | — |
| `marker.letter` | `Section/Letter` | — |
| `double_bar` | `DoubleBar` present | — |
| `tempo_changes[].position` | `Automation[Type=Tempo][Bar=index]/Position` × bar length | Position is a 6-digit float; see §5. |
| `tempo_changes[].bpm` | first number of `Value` | The number printed on the mark, not converted to quarter notes. `Linear`, `Visible` and `Text` → opaque. |
| `tempo_changes[].unit` | second number of `Value`: 1 eighth, 2 quarter, 3 dotted quarter, 4 half, 5 dotted half | Code 0 reads as quarter and is written as 2. Playback converts the mark to quarter notes. |

### Tracks and staves

| Model field | GPIF source | Notes |
|---|---|---|
| `Track.id` | `Track/@id` | id |
| `Track.name` | `Track/Name` | CDATA |
| `Track.short_name` | `Track/ShortName` | CDATA |
| `instrument.kind` | `InstrumentSet/Type`: `drumKit` → Percussion, anything else → Fretted | — |
| `instrument.midi_program` | first `Sounds/Sound/MIDI/Program` | — |
| `tuning.strings` | `Staff/Properties/Property[Tuning]/Pitches`, **reversed** | Model index 0 is the highest string. |
| `capo` | `Property[CapoFret]/Fret` | — |
| `transpose.chromatic`, `.octave` | `Track/Transpose/Chromatic`, `Octave` | Chromatic runs from -12 to 12 and octave from -3 to 3. Observed: chromatic -11 to 0, octave -1 to 0. Guitars and basses carry octave -1. |
| `Track.staves` | `Staves/Staff` | — |
| `Staff.id` | — | The staff has no GPIF id. |
| `Staff.bars` | for master bar i, entry s of `MasterBar[i]/Bars` | s is the staff's global position. |

### Bars, voices, beats

| Model field | GPIF source | Notes |
|---|---|---|
| `Bar.id` | `Bar/@id` | id |
| `Bar.clef` | `Bar/Clef`: `G2` Treble, `F4` Bass, `Neutral` Neutral | Any other clef → opaque. |
| `Bar.voices` | the used slots of `Bar/Voices` | Slots holding `-1` are skipped. The writer puts each voice in its slot and fills the other slots with `-1`. |
| `Voice.id` | `Voice/@id` | id |
| `Voice.slot` | the position (0 to 3) of the voice in `Bar/Voices` | Strictly increasing within a bar. |
| `Voice.beats` | `Voice/Beats` | Shared beats are copied. |
| `Beat.id` | `Beat/@id` | id; one model beat per reference. |
| `Beat.duration.value` | `Rhythm/NoteValue` | — |
| `Beat.duration.dots` | `Rhythm/AugmentationDot/@count` | — |
| `Beat.duration.tuplet` | `Rhythm/PrimaryTuplet` (num, den) | — |
| `Beat.notes` | `Beat/Notes` | Shared notes are copied. |
| `BeatEffects.tremolo_picking` | `Beat/Tremolo`: `1/2` Eighth, `1/4` Sixteenth, `1/8` ThirtySecond | These are the only three values; the model rejects any other note value. |
| `BeatEffects.stroke` | `Property Brush/Direction`, with the duration in XProperty 687935489 | The duration is kept in ticks, 480 per quarter note, from 30 to 480. The start-time XProperty stays opaque and is written back while the stroke is unchanged. |
| `BeatEffects.chord_name` | `Beat/Chord` → `DiagramCollection/Item/@name` | — |
| `BeatEffects.text` | `Beat/FreeText` | CDATA |
| `BeatEffects.fade_in` | `Beat/Fadding` = `FadeIn` | Other values (fade out, volume swell) → opaque. |
| `BeatEffects.grace` | `Beat/GraceNotes`: `BeforeBeat` or `OnBeat` | A grace beat has a rhythm but takes no time in the bar. |
| `BeatEffects.dynamic` | `Beat/Dynamic` | `PPP`, `PP`, `P`, `MP`, `MF`, `F`, `FF`, `FFF`. A missing element reads as `MF`. |
| `BeatEffects.slap` | `Property Slapped` | — |
| `BeatEffects.pop` | `Property Popped` | — |

### Notes

| Model field | GPIF source | Notes |
|---|---|---|
| `Note.id` | `Note/@id` | id; one model note per reference. |
| `NoteKind::Fretted.string` | (string count − 1) − `Property String` | — |
| `NoteKind::Fretted.fret` | `Property Fret` | — |
| `NoteKind::Percussion.articulation` | `InstrumentArticulation` on a drum track | — |
| `NoteEffects.tie_origin` | `Tie/@origin` | — |
| `NoteEffects.tie_destination` | `Tie/@destination` | — |
| `ghost` | `AntiAccent` = `Normal` | — |
| `dead` | `Property Muted` | — |
| `accent` | `Accent` bit 8 → Normal, bit 4 → Heavy | — |
| `staccato` | `Accent` bit 1 | Other bits → opaque. |
| `palm_mute` | `Property PalmMuted` | — |
| `let_ring` | `LetRing` | — |
| `vibrato` | `Vibrato` | `Slight` or `Wide` |
| `hammer_on_pull_off` | `Property HopoOrigin` | `HopoDestination` → opaque. |
| `slide` | `Property Slide/Flags`, one bit | Shift 1, Legato 2, OutDownwards 4, OutUpwards 8, IntoFromBelow 16, IntoFromAbove 32. |
| `harmonic.kind` | `HarmonicType/HType` | — |
| `harmonic.fret` | `HarmonicFret/HFret` (float) | — |
| `bend` | `Bended` and the `Bend…` properties | Four points: origin, middle start, middle end and destination. The two middle points share one value. Offsets run from 0 to 100 and values are in cents (hundredths of a semitone), from -1200 to 1200. |
| `tap` | `Property Tapped` → Right, `Property LeftHandTapped` → Left | — |
| `left_hand_finger` | `LeftFingering` | `Open`, `P` Thumb, `I` Index, `M` Middle, `A` Ring, `C` Little. |
| `right_hand_finger` | `RightFingering` | `P` Thumb, `I` Index, `M` Middle, `A` Ring, `C` Little. |

## 11. GPIF parts not modelled (opaque)

These stay as opaque payloads of their owner, or of the score:

- **Root level:** `GPVersion`, `GPRevision` (mandatory, see §12), `Encoding`, `BackingTrack`, `Assets`, `ScoreViews`.
- **Score:** `WordsAndMusic`, the page header and footer templates, and the system layout and zoom settings.
- **MasterTrack:**
  - `RSE`;
  - the `SyncPoint` automations;
  - the tempo automation's `Linear`, `Visible` and `Text`, and the original text of its `Position`.
- **MasterBar:**
  - `Key/TransposeAs`, `Fermatas`, `TripletFeel`;
  - `Directions`, `XProperties`;
  - a repeat count on a start-only repeat.
- **Track:**
  - colour, layout, playing style and icon;
  - the `InstrumentSet` body (the type is mapped);
  - `NotationPatch`, `RSE`, `ForcedSound`;
  - all but the first sound's program;
  - `MidiConnection`, playback and engine state, `Lyrics`;
  - track automations and `LetRingThroughout`.
- **Staff properties:**
  - `FretCount`, partial capo;
  - the tuning extras (`Flat`, `Instrument`, `Label`, `LabelVisible`) and `TuningFlat`;
  - the chord and diagram collections and working sets;
  - the tuning name.
- **Bar:** `SimileMark`, `XProperties`, and any clef other than `G2`, `F4` and `Neutral`.
- **Beat:**
  - the stem orientation elements;
  - `Whammy`, `Variation`, `Wah`, `Golpe`, `Slashed`, `Hairpin`, `Legato`, `Arpeggio`, `Lyrics`;
  - the pickup properties, `PickStroke`, the barre properties, `Rasgueado`, `VibratoWTremBar`;
  - the brush start time and the remaining `XProperties`;
  - `Fadding` values other than `FadeIn`.
- **Note:**
  - `Ornament`;
  - Accent bits 2 and 16;
  - `ShowStringNumber`, `HopoDestination`;
  - slide and accent bit combinations that a single model value cannot hold;
  - the pseudo string and fret of a drum note;
  - `Midi`, `ConcertPitch` and `TransposedPitch`. These are derived, but must be written back (§9). A writer replays the original values while string, fret, tuning and transpose are unchanged, and computes them otherwise.
  - The note's `InstrumentArticulation` on pitched tracks.

## 12. Minimal GPIF

We wrote a one-track, one-bar score from scratch and removed parts one at a time. GP opened the following and exported it to MusicXML. The shape is schematic; the values are ours:

```
<GPIF>
  <GPRevision required="…" recommended="…">…</GPRevision>
  <Score><Title><![CDATA[…]]></Title></Score>
  <MasterTrack><Tracks>0</Tracks></MasterTrack>
  <Tracks><Track id="0">
    <Name><![CDATA[…]]></Name><ShortName><![CDATA[…]]></ShortName>
    <InstrumentSet><Type>electricGuitar</Type></InstrumentSet>
    <Staves><Staff><Properties>
      <Property name="Tuning"><Pitches>p p p p p p</Pitches></Property>
    </Properties></Staff></Staves>
  </Track></Tracks>
  <MasterBars><MasterBar>
    <Key><AccidentalCount>0</AccidentalCount><Mode>Major</Mode></Key>
    <Time>4/4</Time><Bars>0</Bars>
  </MasterBar></MasterBars>
  <Bars><Bar id="0"><Clef>G2</Clef><Voices>0 -1 -1 -1</Voices></Bar></Bars>
  <Voices><Voice id="0"><Beats>0</Beats></Voice></Voices>
  <Beats><Beat id="0"><Rhythm ref="0"/><Notes>0</Notes></Beat></Beats>
  <Notes><Note id="0"><Properties>
    <Property name="String"><String>s</String></Property>
    <Property name="Fret"><Fret>f</Fret></Property>
  </Properties></Note></Notes>
  <Rhythms><Rhythm id="0"><NoteValue>Whole</NoteValue></Rhythm></Rhythms>
</GPIF>
```

What each part does:

- **`GPRevision` is mandatory.** Without it, GP neither opens the file nor shows any message: the open is silently ignored [m2].
- **`InstrumentSet` is mandatory.** A score without it crashed GP [m1]; the same score with a revision and a set opened [m3]. A set with only `Type` is enough; GP fills in the rest on save [i-iset-type-only].
- **Everything else is optional:**
  - `GPVersion`, `Encoding`;
  - the other `Score` fields;
  - tempo (GP defaults to 120);
  - colour, layout and playing style;
  - `Transpose`, `Sounds`, `MidiConnection`, playback state;
  - `CapoFret`, `FretCount` and the tuning extras;
  - track automations, `Key/TransposeAs`, `Dynamic`, `InstrumentArticulation`.

  GP fills defaults for all of these on save: `MF` dynamics, `TransposeAs Flats`, pickup properties, stem orientations and the derived pitch properties.

## 13. What GP changes on re-save

These are the normalisations we observed. Byte equality with a source file is not expected after a round trip, so a fidelity check must tolerate them.

- **Unchanged examples.** Re-saving keeps `score.gpif` byte-identical in one example [e4-pcempty]. In two others [r-ex1, r-ex2], it **clears the tuning name** of some staves (a drum track's dummy tuning name, and a bass's `E A D G`). `BinaryStylesheet` changes bytes but not length.
- **Sharing.** GP rebuilds beat and note sharing from scratch (§3.1).
- **Tempo.** Unit code 0 becomes 2.
- **Missing optional elements** are filled with defaults (§12). Missing directory entries and container entries are written again.
- **Element and property order** is GP's own: beat children as in §8, note properties as GP lists them. Our out-of-order input came back in GP's order.
- **Text.** Values written without CDATA become empty (§2.1). That is data loss, not normalisation.
