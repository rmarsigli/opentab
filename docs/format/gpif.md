# The Guitar Pro 7/8 `.gp` format, as OpenTab needs it

This is a clean-room description. It is derived only from two sources: files that Guitar Pro 8.1.5 (build 31, GPIF revision 13008) wrote itself, and experiments in which we handed GP 8 files we built and watched whether it opened them, what it showed, and what it wrote back on "Save As". No vendor documentation, source code or third-party parser was consulted. Snippets below are schematic: element names are real, every value is a placeholder.

Words used in this document:

- **GP** means Guitar Pro 8.1.5.
- **Accepted** means GP opened the file with no error notice and did not crash.
- **Re-save** means GP opened a file and saved it with "Save As" into a new path.
- **Experiment files** named in brackets, such as [min-all], are kept locally in the oracle machine's experiments folder (`C:\gf-oracle\corpus\experiments\`). They are not part of the repository; a bracketed name identifies the experiment files whose names start with it. Names that start with `m3-`, such as [m3-po07], live in its `m3-t1` subfolder without that prefix: `inputs\po07-*.gp` is the file we built, and `harvests\po07-*\` holds what GP exported from it.

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

How OpenTab writes the archive: the entries in container order, files deflated at level 6, directories stored and empty, no data descriptors, a 1980-01-01 timestamp, no extra fields and no comments, "made by" Unix with modes 0644 and 0755. The writer builds the zip bytes itself and deflates with `miniz_oxide`, pinned to an exact version, so the bytes never depend on which `flate2` backend another crate of the build turns on. A test writes every committed fixture back byte for byte.

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

In the 27 examples, the first part lists every track and one more part follows per track; every flag byte is 0, drum tracks carry 1 (notation only) and the trailing number is 1 in 25 files and 0 in 2. Drum tracks do not need it: a score with a drum kit opens with the empty form (§16).

### 1.2 Entries of our own

GP opens a container that holds an extra entry it does not know, with no notice, and exports it normally. We tried a JSON entry under `Content/` in a subfolder, a JSON entry at the archive root and a 256-byte binary entry under `Content/` [m3-x01, m3-x02, m3-x03]. **GP's re-save drops the entry**: the re-saved container holds only GP's own fixed list of entries (§1). Data that OpenTab keeps in such an entry therefore survives our own round trips and a file opened in GP, but not a file that GP saves again. The committed fixture `sound-override.gp`, written by our writer with this entry, behaves the same: GP opened it with no notice, every export ran, and the re-save holds no OpenTab entry [sound-override].

OpenTab keeps one such entry: `Content/OpenTab/sound-overrides.json`, holding the per-score sound bank overrides as `{"sound_overrides": [{"track": T, "bank": "B"}]}`, where `T` is the track's position in `Tracks`. The entry belongs to the writer: it drops any container entry of that name it is given and adds its own at the end of the container only when the score has an override; the reader takes it out of the container again. A malformed entry, or one that names a missing track or a track twice, is a read error.

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

MusicXML playback tempo is converted to quarter notes. For example, 80 with unit 1 plays at 40 quarter notes per minute, and 80 with unit 3 at 120.

GP's MIDI export converts the mark differently for one unit: **a dotted quarter counts as 1.25 quarters**, not 1.5. Its set-tempo event holds `floor(60 000 000 / (BPM × f))` microseconds per quarter, with `f` = 0.5, 1, 1.25, 2 and 3 for units 1 to 5. Every tempo event of the `tempo` and `tempo-ramp` goldens follows it: a dotted-quarter 100 plays 480 000 µs (125 quarters), a dotted-quarter 80 600 000 µs (100 quarters), 90 plays 666 666 µs and 140.5 plays 427 046 µs [tempo, tempo-ramp, m3-t7-ramp-dir]. `oracle_fidelity` checks this formula against both goldens.

`Position` is written with at most six significant digits, for example `0.333333`. It is not an exact rational.

With no tempo automation at all, GP plays the score at 120: the MusicXML export of a score without one carries a quarter = 120 metronome mark [min-all].

The other master automation type seen is `SyncPoint`. It aligns bars with a backing-track audio file and appears only in scores that have assets. Its `Value` holds sub-elements (bar index, occurrence, frame offset, tempi).

**A sync point's `Bar` counts bars in playback order, not in the score.** `Value/BarIndex` is the master bar index and `Value/BarOccurrence` is the pass through that bar, counting from 0. In 5 examples, 546 sync points have `Bar` larger than the number of master bars, or different from `BarIndex`. Every one of them equals the position of (`BarIndex`, `BarOccurrence`) in the playback order that this rule produces:

1. Bars play from left to right. A bar with a repeat start opens a section and resets the pass to 1, unless playback just jumped back to it.
2. A bar with `AlternateEndings` starts an ending group. The group runs through the following bars that have no endings of their own, and stops at the next repeat start. On pass p, a bar in a group whose endings do not include p is skipped.
3. At a repeat end with count c, while the pass is below c, the pass grows by one and playback jumps back to the start of the section, or to the first bar if no section was opened.

None of these 5 examples has `Directions`, so the rule ignores them; §5.1 adds them. The other automations in GPIF (tempo, the track's `Sound` automations and the `DSPParam` automations under `RSE`) all name a plain master bar index in `Bar`.

### 5.1 Play order with directions

We learned GP's order by building scores in which every bar holds one whole note of its own pitch, adding repeats and `Directions` (§6.1), and reading the note order of GP's MIDI export. Bars below are 0-based master bar indices. The rules hold for every one of the 54 experiments [m3-po01] to [m3-po59], for the 12 committed `directions-*` fixtures (their GP orders are the predicted orders in `xtask/src/fixtures/play-orders.json`, checked against `export.mid` by `oracle_fidelity`) for the 15 experiments [m3-t4-x1] to [m3-t4-y3] and for the 4 look-ahead experiments [m3-fw-l1] to [m3-fw-l4]:

1. **Bars and repeats** play as in the rule above.
2. **Targets** mark the start of their bar. **Jumps** act at the end of their bar, after it is played. GP writes the targets of a bar before its jumps.
3. **Return jumps** are every `DaCapo…`, `DaSegno…` and `DaSegnoSegno…` jump, with or without a suffix:
   - `DaCapo…` goes to bar 0, `DaSegno…` to the bar holding `Segno`, `DaSegnoSegno…` to the bar holding `SegnoSegno`;
   - once fired, the suffix `AlFine` enables the `Fine` (rule 8), `AlCoda` enables every `DaCoda` and `AlDoubleCoda` every `DaDoubleCoda` (rule 9, condition 1).
4. **Every jump fires at most once** in a play-through, so the expansion is always finite. A jump reached again after it fired is passed over [m3-po01, m3-po05, m3-po25, m3-po37].
5. **A return jump inside a repeated section fires only on the section's last pass.** The section runs from the repeat start to the repeat end, inclusive. A return jump on the repeat-end bar fires after the last pass [m3-po15, m3-po31, m3-po34]. One in an alternate ending that the last pass skips never fires [m3-po39].
6. **After a return jump every repeat plays again in full**: pass counts restart, and alternate endings start again from ending 1 [m3-po13, m3-po14, m3-po23, m3-po27].
7. **A return jump whose target does not exist is ignored**, for example `DaSegno` or `DaSegnoAlCoda` without a `Segno` [m3-po19, m3-po28].
8. **`Fine`** acts only after an `…AlFine` jump has fired. It then ends playback at the end of its bar, on the first pass that reaches it, even inside a repeated section [m3-po02, m3-po06, m3-po10, m3-po24, m3-po32]. With only a plain `DaCapo`, or with no jump at all, it is ignored [m3-po17, m3-po30].
9. **`DaCoda` (To Coda) and `DaDoubleCoda` (To Double Coda)** jump to the bar holding `Coda` or `DoubleCoda`. They are ignored when that bar does not exist [m3-po20] or does not lie after the jump's bar [m3-po46]. Otherwise a `DaCoda` in bar b fires at the end of a visit to b when either condition holds:
   1. an `…AlCoda` jump has already fired [m3-po03, m3-po07, m3-po11, m3-po16, m3-po26, m3-po33, m3-po37, m3-po38];
   2. **the look-ahead from b finds nothing that holds it back** [m3-po18, m3-po28, m3-po42, m3-po43, m3-po51, m3-po52, m3-po54, m3-po58, m3-po59].

   **The look-ahead** follows the bars that playback would play after this visit. It starts with bar b's own jumps [m3-po21], then follows rules 1 to 8 and the pass counts of this moment, until the end of the score. Jumps on that path fire as they would, including a forward `DaSegno` [m3-po52]. It holds the `DaCoda` back on the first bar it reaches that holds either of these:
   - a return jump of any kind (`…AlCoda`, `…AlDoubleCoda`, `…AlFine` or no suffix) that has not fired, whose target bar exists and lies at b or before it [m3-po44, m3-po54, m3-po56, m3-po59]. The jump only needs to be reached, not to fire there: in [m3-po51] the al-Coda jump sits in ending 1 of a repeat. On pass 1 it is ahead, so the `DaCoda` waits even though rule 5 stops the jump from firing on that pass. On pass 2 the ending is skipped, so the `DaCoda` fires;
   - a `DaDoubleCoda` that has not fired and is not ignored, so its `DoubleCoda` bar exists and lies after the `DaDoubleCoda`'s own bar, as the first sentence of this rule requires [m3-po41]. This applies to `DaCoda` only. An ignored `DaDoubleCoda` holds nothing: one whose `DoubleCoda` lies before it, or in its own bar, let a `DaCoda` behind it fire on its first visit, while a valid one in the same place held it back until the `DaDoubleCoda` fired [m3-t4-y1, m3-t4-y2, m3-t4-y3].

   A jump that merely exists does not count. An al-Coda jump earlier in the score [m3-po42], one jumped over by a forward `DaSegno` [m3-po52], one whose target lies after b [m3-po43] or one without its target [m3-po28] lets the `DaCoda` fire on its first visit.

   **`DaDoubleCoda`** follows the same rule with `…AlDoubleCoda` in condition 1 and only the return-jump case in the look-ahead [m3-po04, m3-po12, m3-po26, m3-po40, m3-po55]. An unfired `DaCoda` ahead does not hold it back [m3-po55]. A firing `DaCoda` or `DaDoubleCoda` leaves its bar before that bar's own repeat end [m3-po16, m3-po33, m3-po38].

   **Directions met on the path.** A `Fine` does not stop the look-ahead, whether or not an `…AlFine` jump has enabled it: the path runs on past it, and a return jump behind it still holds the jump back [m3-fw-l1, m3-fw-l2, m3-fw-l4]. A `DaCoda` met on a `DaDoubleCoda`'s path that an `…AlCoda` jump has already enabled fires there and redirects the path to the `Coda`, so a return jump it skips holds nothing [m3-fw-l3]. Still untested, and treated as passive on the path: a forward jump that no `…Al` jump has enabled yet, which would need a look-ahead of its own.
10. **Two directions in one bar** follow the same rules one by one. A `DaCoda` held back by a return jump in its own bar is passed over while that jump fires; on the next visit it fires [m3-po21]. A bar holding `Fine` and an `…AlFine` jump fires the jump first and ends at the `Fine` on the next visit [m3-po22].
11. **Unusual placements:**
    - A `DaSegno` whose `Segno` lies after it jumps forward to it [m3-po45].
    - **GP keeps only the last copy of each direction name in the score.** On open it drops every earlier copy of a target or a jump, so its re-save no longer holds them and playback follows the last copy alone. Two `Segno` bars, two `Fine` bars [m3-po47, m3-po48, directions-duplicates], two `DaCapo` [directions-endings] and two `DaCoda` [directions-coda-free, m3-t4-x1, m3-t4-x3], as well as two `DaDoubleCoda` [m3-t4-x5], all re-saved with the last copy only. In [directions-duplicates] the `DaSegnoAlFine` in bar 3 therefore jumps forward to the `Segno` in bar 4 and ends at the `Fine` in bar 5. In [directions-coda-free] the remaining `DaCoda` lies after its `Coda`, so it is ignored and the forward `DaSegno` is played.
    - A return jump in a second ending that lies after the repeat end is outside the repeated section, so it fires on its first visit [m3-po49].
12. **A `DaCoda` or `DaDoubleCoda` that playback never visits truncates GP's export.** When the bar holding one, with its target after it, lies on no path that playback takes (a forward `DaSegno` jumps over it [m3-t4-x7, m3-t4-x10, m3-t4-x12], or a `DaDoubleCoda` that fired jumps over it [m3-t4-x11]), GP's MIDI export holds only the bars from that target to the end of the score. Every bar before the target is missing. [m3-t4-x1] and [m3-t4-x5] show the same thing after GP drops their earlier twin. This looks like a defect of GP's export rather than a play rule. A `gp` profile that mirrors the export must decide whether to copy it; no committed fixture triggers it.

Selected orders (bar: directions → order played):

| Experiment | Bars | Played |
|---|---|---|
| [m3-po02] D.C. al Fine | 2 `Fine`, 4 `DaCapoAlFine` (6 bars) | 0 1 2 3 4 0 1 2 |
| [m3-po07] D.S. al Coda | 1 `Segno`, 2 `DaCoda`, 3 `DaSegnoAlCoda`, 4 `Coda` (6 bars) | 0 1 2 3 1 2 4 5 |
| [m3-po12] D.S.S. al Double Coda | 1 `SegnoSegno`, 2 `DaDoubleCoda`, 3 `DaSegnoSegnoAlDoubleCoda`, 4 `DoubleCoda` (6 bars) | 0 1 2 3 1 2 4 5 |
| [m3-po13] repeats after D.C. | repeat 1–2, 4 `DaCapo` (6 bars) | 0 1 2 1 2 3 4 0 1 2 1 2 3 4 5 |
| [m3-po14] endings after D.S. | 1 `Segno` + repeat start, 2 ending 1 + repeat end, 3 ending 2, 4 `DaSegno` (6 bars) | 0 1 2 1 3 4 1 2 1 3 4 5 |
| [m3-po15] D.C. inside a repeat | repeat 1–3, 2 `DaCapo` (5 bars) | 0 1 2 3 1 2 0 1 2 3 1 2 3 4 |
| [m3-po18] To Coda alone | 1 `DaCoda`, 3 `Coda` (5 bars) | 0 1 3 4 |
| [m3-po26] Coda then Double Coda | 1 `Segno`, 2 `DaCoda`, 3 `DaDoubleCoda`, 4 `DaSegnoAlCoda`, 5 `Coda`, 6 `DaSegnoAlDoubleCoda`, 7 `DoubleCoda` (9 bars) | 0 1 2 3 4 1 2 5 6 1 2 3 7 8 |
| [m3-po31] D.S. inside a repeat ×3 | 1 `Segno`, repeat 2–4 ×3, 3 `DaSegno` (6 bars) | 0 1 2 3 4 2 3 4 2 3 1 2 3 4 2 3 4 2 3 4 5 |
| [m3-po33] To Coda inside a repeat | repeat 0–2, 1 `DaCoda`, 3 `DaCapoAlCoda`, 4 `Coda` (6 bars) | 0 1 2 0 1 2 3 0 1 4 5 |
| [m3-po41] Double Coda holds To Coda back | 1 `DaCoda`, 2 `DaDoubleCoda`, 3 `DaCapoAlDoubleCoda`, 4 `Coda`, 5 `DoubleCoda` (7 bars) | 0 1 2 3 0 1 2 5 6 |
| [m3-po42] al Coda behind the To Coda | repeat 0–1, 1 ending 1 + `DaCapoAlCoda`, 2 ending 2, 3 `DaCoda`, 5 `Coda` (7 bars) | 0 1 0 2 3 5 6 |
| [m3-po44] al Coda after the Coda | 1 `DaCoda`, 3 `Coda`, 5 `DaCapoAlCoda` (7 bars) | 0 1 2 3 4 5 0 1 3 4 5 6 |
| [m3-po51] al Coda in a skipped ending | repeat 0–2, 1 `DaCoda`, 2 ending 1 + `DaCapoAlCoda`, 3 ending 2, 4 `Coda` (6 bars) | 0 1 2 0 1 4 5 |
| [m3-po52] al Coda jumped over | 1 `DaCoda`, 2 `DaSegno`, 3 `DaCapoAlCoda`, 4 `Segno`, 5 `Coda` (7 bars) | 0 1 5 6 |
| [m3-po54] al Fine ahead | 1 `DaCoda`, 3 `Coda`, 4 `Fine`, 5 `DaCapoAlFine` (7 bars) | 0 1 2 3 4 5 0 1 3 4 |
| [m3-po58] To Coda after al Double Coda | 1 `DaCoda`, 2 `DaCapoAlDoubleCoda`, 3 `Coda`, 4 `DoubleCoda` (6 bars) | 0 1 2 0 1 3 4 5 |
| [directions-coda-free] twin To Coda | 1 `DaCoda`, 2 `DaSegno`, 3 `DaCapoAlCoda`, 4 `Segno`, 5 `Coda`, 6 `DaCoda` (8 bars) | 0 1 2 4 5 6 7 |
| [m3-t4-x7] To Coda jumped over | 2 `DaSegno`, 3 `DaCoda`, 4 `Segno`, 5 `Coda` (8 bars) | 5 6 7 |
| [m3-t4-y1] ignored Double Coda ahead | 1 `DoubleCoda`, 2 `DaCoda`, 3 `DaDoubleCoda`, 4 `Coda` (8 bars) | 0 1 2 4 5 6 7 |
| [m3-fw-l1] active Fine ahead of a To Coda | 1 `DaCapoAlFine`, 2 `DaCoda`, 3 `Fine`, 4 `DaCapo`, 5 `Coda` (8 bars) | 0 1 0 1 2 3 |
| [m3-fw-l2] idle Fine ahead of a To Coda | 1 `DaCoda`, 2 `Fine`, 3 `DaCapo`, 4 `Coda` (8 bars) | 0 1 2 3 0 1 4 5 6 7 |
| [m3-fw-l3] enabled To Coda ahead of a To Double Coda | 1 `DaDoubleCoda`, 2 `DaCoda`, 3 `DaCapoAlCoda`, 4 `DaCapoAlDoubleCoda`, 5 `Coda`, 6 `DoubleCoda` (8 bars) | 0 1 2 3 0 1 6 7 |
| [m3-t4-y3] valid Double Coda ahead | 1 `DaCoda`, 2 `DaDoubleCoda`, 4 `Coda`, 6 `DoubleCoda` (8 bars) | 0 1 2 6 7 |

### 5.2 Tempo ramps

`Linear` is a flag on a tempo automation. 4 of the 39 tempo automations in the corpus have it set to `true`, and 3 of those are the last tempo of their score, so nothing follows them. GP keeps the flag on re-save [m3-mx06, tempo-ramp].

**Direction: seen in GP's notation.** GP's exports carry no ramp:
- its MIDI export writes one tempo event at each automation and nothing between them;
- its MusicXML export carries only the tempo at each point [m3-mx06, m3-mx08], so neither shows a direction.

GP's engraving does show one, in the PNG:
- [m3-mx06]: a `Linear` 60 in bar 0 followed by 120 in bar 2 is drawn as the 60 mark, then "accel" with a dashed line running to the 120 mark;
- [m3-mx08]: `Linear` 100, then `Linear` 200 at the middle of bar 1, then 50 in bar 3. GP drew "accel" dashed from 100 to 200, then "rall" dashed from 200 on to 50.

So `Linear true` starts a ramp at its own automation and ends it at the next tempo automation of the score. GP labels the ramp by whether the tempo rises or falls **in quarter notes per minute**, not by the numbers of the marks: a dotted-quarter 100 ramping to a quarter 120 (150 to 120 quarters) and a dotted-quarter 80 ramping to an eighth 160 (120 to 80 quarters) are both drawn "rall", although each mark's number rises [m3-t7-ramp-dir]. The `tempo-ramp` golden draws "accel" from 60 to 120, "rall" from 120 to 90 and "accel" from a dotted-quarter 80 to 140.5, which GP prints as 141 [tempo-ramp].

**GP's exports ignore the ramp.** The `tempo-ramp` golden settles it for M3 ledger Ruling 2:
- `export.mid` holds exactly one tempo event per tempo automation, at the automation's bar fraction, and nothing between them. Each tempo holds until the next automation, linear or not [tempo-ramp, m3-t7-ramp-dir]. There is no step grid.
- `export.musicxml` carries one metronome mark and one `<sound tempo>` per automation, and nothing for the ramp.
- `export.gp5` carries the tempo of each automation only.

So the `gp` playback profile does not ramp: it holds each tempo constant until the next tempo automation. With the discretised ramp of `opentab-playback` (a step of `RampGrid` whole notes, the first step at the ramp's starting tempo), that is a step longer than any score, for example `RampGrid(Fraction::new_raw(i64::MAX, 1))`: a ramp then never leaves its first step.

**Model.** The flag is `TempoChange.linear` (§10). `<Linear>true</Linear>` is written only on a ramp; GP's own `<Linear>false</Linear>` is kept, guarded by the change's position and flag, and dropped when the flag turns on. A `Linear` the model cannot read as `true` or `false` keeps its text under the same guard. A `Linear` on the last tempo of the score ramps to nothing, so playback holds that tempo; it is not diagnosed (3 corpus files have one).

**Exact tempo.** `Tempo` stores the BPM as the `f64` the reader parsed, and the reader never rounds it. `Tempo::exact()` reads the shortest decimal that prints back to that `f64` as an exact rational, with checked arithmetic, and accepts up to six decimals; a value with more (for example a 17-digit JSON tempo) has no exact value and is diagnosed as `inexact_tempo`; `set_tempo` and `restore_tempo_changes` refuse it. Every corpus tempo is exact: the most precise has four decimals. Playback reads tempo only through `exact()`.

**Shape: declared assumption.** No oracle shows how the tempo moves inside the ramp: linear in beats per minute against score time, or anything else. GP's own playback was not measured. OpenTab assumes a linear change in beats per minute over score position from one point to the next. A later harvest's PNG can only confirm the direction again, not the shape.

## 6. Master bars

```
<MasterBar>
  <Key><AccidentalCount>K</AccidentalCount><Mode>Major|Minor</Mode><TransposeAs>Sharps|Flats</TransposeAs></Key>
  <Time>N/D</Time>
  <Fermatas>…</Fermatas>                         optional
  <Repeat start="true|false" end="true|false" count="C"/>   optional
  <Section><Letter><![CDATA[..]]></Letter><Text><![CDATA[..]]></Text></Section>   optional
  <AlternateEndings>E E</AlternateEndings>       optional
  <TripletFeel>Triplet8th</TripletFeel>          optional
  <DoubleBar/>                                   optional
  <Directions><Target>..</Target><Jump>..</Jump></Directions>   optional
  <Bars>b b</Bars>
  <XProperties>…</XProperties>                   optional
</MasterBar>
```

- `AccidentalCount` runs from -7 to 7. Negative counts are flats.
- `Repeat` combines a start flag, an end flag and a play count (`count`). The count is 2 on repeat ends and usually 0 on starts. Three starts carry 2.
- `AlternateEndings` lists the ending numbers that cover this bar. The corpus only has single values (1 or 2).
- `Section` is a rehearsal mark: a letter and a text. Either may be empty. The letter is the mark shown beside the text.
- `DoubleBar` marks a double bar line at the end of the bar.
- `TripletFeel` swings the bar: `Triplet8th`, `Triplet16th`, `Dotted8th`, `Dotted16th`, `Scottish8th` or `Scottish16th`. GP8 opened a file of ours with each name and re-saved it unchanged [m3-t11-probe]. The corpus holds 177 bars with `Triplet8th` in 6 files and no other name. GP's MIDI export plays the swing (§17).
- `Directions` holds segno and coda targets and jumps such as `DaCoda`, `DaSegno` and `DaSegnoAlCoda`; see §6.1.
- Child order, as GP writes it: `Key`, `Time`, `Repeat`, `Section`, `AlternateEndings`, `TripletFeel`, `DoubleBar`, `Directions`, `Bars`, `XProperties`. No pair of master bar children breaks that order in the 1 260 master bars of the corpus. `Fermatas` comes after `Time` and before `Bars`; it never shares a bar with `Repeat`, `Section`, `AlternateEndings`, `TripletFeel`, `DoubleBar` or `Directions`.

### 6.1 Directions

```
<Directions>
  <Target>NAME</Target>   zero or more
  <Jump>NAME</Jump>       zero or more
</Directions>
```

The corpus has 8 master bars with `Directions`, in 2 files: 4 `Target` (`Segno` 2, `Coda` 2) and 4 `Jump` (`DaCoda` 2, `DaSegno` 1, `DaSegnoAlCoda` 1). 3 of the 8 bars also hold a `Repeat`. We wrote every other name below into scores of our own; GP opened each and re-saved every name unchanged [m3-po01] to [m3-po41]:

| Kind | Names |
|---|---|
| Target | `Segno`, `SegnoSegno`, `Coda`, `DoubleCoda`, `Fine` |
| Jump back to bar 0 | `DaCapo`, `DaCapoAlFine`, `DaCapoAlCoda`, `DaCapoAlDoubleCoda` |
| Jump back to `Segno` | `DaSegno`, `DaSegnoAlFine`, `DaSegnoAlCoda`, `DaSegnoAlDoubleCoda` |
| Jump back to `SegnoSegno` | `DaSegnoSegno`, `DaSegnoSegnoAlFine`, `DaSegnoSegnoAlCoda`, `DaSegnoSegnoAlDoubleCoda` |
| Jump forward | `DaCoda` (To Coda), `DaDoubleCoda` (To Double Coda) |

OpenTab models them as `MasterBar.directions` (§10). Diagnostics flag a jump whose destination or `Al…` target is missing from the score (`dangling_jump`, one per missing target), every copy of a target or a jump name except the last one in the score (`duplicate_target` and `duplicate_jump`, each on the bar of a copy GP drops; GP keeps only the last copy, §5.1 rule 11; a name repeated inside one bar counts too), and a `DaCoda` or `DaDoubleCoda` with no copy of its target in a later bar (`coda_not_ahead`; GP ignores such a jump, rule 9). GP accepts all four, so the writer refuses none.

A bar may hold several targets and jumps. GP writes the targets first and orders jumps its own way: we wrote `DaCoda` before `DaSegnoAlCoda`, and the re-save has them the other way round [m3-po21]. Our writer emits GP's order, so none of the 12 `directions-*` fixtures came back reordered [directions-ds-coda]. The play order they produce is in §5.1.

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

- **InstrumentSet/Type** names the instrument family. The corpus has these types, with their track counts (64 tracks):
  - stringed: `electricGuitar` 16, `steelGuitar` 12, `nylonGuitar` 6, `electricBass` 6, `acousticBass` 4, `violin` 1;
  - other pitched: `acousticPiano` 2, `saxophone` 2, `trumpet` 2, `electricOrgan` 2, `voice` 1;
  - percussion: `drumKit` 10.

  Every set has the children `Name`, `Type`, `LineCount` (always 5) and `Elements`. A drum kit lists its instruments and their articulations (54 elements and 95 articulations in each drum track of the corpus; §16). A pitched instrument lists a single element of type `pitched` with one articulation whose `OutputMidiNumber` is 0 (54 tracks).
- **Every non-drum track is stored as a string instrument.** Piano, saxophone, trumpet, organ and voice tracks all carry a six-string tuning, and their notes carry string and fret. Drum tracks carry a dummy tuning of six zeros.
- **Tuning** `Pitches` are MIDI numbers, **lowest string first**. The corpus has these tunings:
  - guitar: standard, drop D, and a half step down;
  - bass: 4-string, standard and others;
  - violin: G D A E.
- **CapoFret** is the capo fret. Two tracks have a non-zero capo.
- **Transpose** is the written-to-sounding offset used for standard notation. Guitars and basses carry `Octave -1`, because they are written an octave above concert pitch. Transposing instruments and detuned guitars also carry a `Chromatic` offset. 49 of 64 tracks have a non-zero transpose.
- **Sounds/Sound/MIDI/Program** is the General MIDI program. Two tracks have two sounds, and `Sound` automations on the track switch between them over time. Sounds, MIDI channels, the mixer and track automations are described in §15.
- **Lyrics/Line/Offset** is read as the master bar index where the line starts. GP's lyrics dialog starts each line at a bar. All 320 offsets in the corpus are 0, so the meaning has not been tested against GP.
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
| Whammy bar (tremolo bar) | `<Whammy>`, whose attributes give a four-point curve; see below. |
| Arpeggio | `<Arpeggio>`. |
| Hairpins | `<Hairpin>`, `Crescendo` or `Decrescendo`. |
| Slurs across beats | `<Legato origin destination>`. |
| Free text | `<FreeText>`. |

**Whammy bar.** `<Whammy originValue="V" middleValue="V" destinationValue="V" originOffset="O" middleOffset1="O" middleOffset2="O" destinationOffset="O"/>`, every number written with six decimals. It has the bend's four-point shape (§9): an origin, two middle points sharing one value, and a destination; offsets run from 0 to 100 across the beat. Values count hundredths of a tone, like bends: GP draws 100 as a dive or rise of "1" (a whole tone) and -100 as "-1" [m3-mx07], so the model value in cents is the GPIF value × 2. Negative values dive. OpenTab types it as `BeatEffects.tremolo_bar` (§10), with the bend's limits (offsets 0 to 100, values -1200 to 1200 cents). A value that does not become a whole number of cents in that range, an offset that is not a whole number from 0 to 100, or a missing attribute cannot be held by the model, so the element stays opaque and is replaced when a tremolo bar is set. An element in another spelling than the writer's (for example `0` instead of `0.000000`) is typed and also kept, guarded by the curve, so it is written back unchanged until the curve changes. The corpus has 2 whammy beats in 2 files, with values 0, -50 and -100 and offsets 0, 35, 50, 99 and 100. The separate beat property `VibratoWTremBar` (1 beat) is a vibrato played with the bar.

What GP does with a whammy we wrote:
- **It keeps values beyond the model's range.** ±300, ±600, ±700 and ±1200 came back unchanged in the re-save and the MusicXML [m3-t7-tb-range]. The model holds ±600 (±1200 cents), so larger ones stay opaque. The 12.5 middle value and split middle offsets (25 and 75) also survive [tremolo-bar].
- **It continues the bar from the previous beat.** When the previous beat of the same voice has a whammy, GP replaces this beat's `originValue` with the previous `destinationValue`, across bar lines too: 0 became -100, -100 became 50 and 0 became -75 [tremolo-bar]. A beat after a beat without a whammy keeps its origin (a pre-dive at -100 stayed), and a beat without a whammy gets none [m3-t7-tb-cont].
- **Its re-save shares beats that differ only in the whammy's middle value.** Two beats with the same notes, rhythm, offsets, origin and destination are written as one beat, and the later one takes the earlier one's curve: middles -50 and -25 came back as -100 [m3-t7-tb-share2, m3-t7-tb-share]; in [tremolo-bar] the last beat (0, 0, 0, continued to -200, 0, 0) came back as the second beat's -100, -100, 0. The origin compared is the one written in the file, before the continuation above. Beats that differ in origin or destination stay apart [m3-t7-tb-cont]. This loses data; GP's MusicXML export, made before the save, still shows the real values.
- **Exports.** The MIDI export plays the notes without any pitch bend (§17). The MusicXML export writes a processing instruction per note, `<?GP <root><WhammyBar data='{"destination":"D","middle":"M","origin":"O"}'/></root>?>`, with the continued origin and without offsets. The GP5 export approximates each curve with GP5's tremolo-bar points.

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

**Trill.** A trilled note carries `<Trill>P</Trill>` as a note child, and usually a note `XProperty` for its speed:

```
<Note id="n">
  <Trill>P</Trill>
  <InstrumentArticulation>0</InstrumentArticulation>
  <Properties>…</Properties>
  <XProperties><XProperty id="688062467"><Int>T</Int></XProperty></XProperties>
</Note>
```

- `P` is the sounding MIDI number of the auxiliary note: open string + capo + auxiliary fret. OpenTab always counts from the track's tuning and capo (`Track::trill_pitch_of`), also for a later staff that carries a tuning of its own, so the reader, the writer, diagnose and the commands share one rule. GP's dialog asks for the auxiliary fret. Fret 2 on an open E string (64) gave 66 and fret 5 gave 69 [m3-tr02, m3-tr04]; with capo 2, fret 2 gave 68 [m3-tr23].
- `T` is the length of each trill note in ticks at 480 per quarter note. GP's dialog wrote 480 by default [m3-tr02]. We wrote 60, 120, 240 and 480, and GP alternated the notes every 60, 120, 240 and 480 ticks in its MIDI export [m3-tr12, m3-tr13]. The dialog's speed slider shows four note values, from thirty-second to quarter. Without the property GP plays 60 and does not add the property on re-save [m3-tr20].
- **Exports** [trill]. The MIDI export alternates principal and auxiliary every `T` ticks from the note's onset, starting with the principal, `length / T` notes per trilled note, all at the beat's velocity; the auxiliary note-offs name the principal key (§17). The MusicXML export writes only `<ornaments><trill-mark trill-step="whole"/></ornaments>`, with no speed and no auxiliary pitch. The GP5 export writes the auxiliary fret and a period code.
- **A capo raises the revision.** A score with a trilled note on a track with a capo is re-saved with `GPRevision required="12025"` instead of 12024, with or without the speed property; the same trills with capo 0 keep 12024 [trill, m3-t7-trill-capo0, m3-t7-trill-nospeed]. Files we write with 12024 open.
- **Child order.** GP's re-save writes `Trill` before every other note child, `InstrumentArticulation` included, on all four notes of the [trill] golden; `oracle_fidelity` checks it.
- **960 made GP hang** during the MIDI export: the process stopped responding and grew to about 10 GB of memory, and had to be killed [m3-tr13-speed-960]. A writer must write only 60, 120, 240 or 480.
- The corpus holds no trill.
- OpenTab types it as `NoteEffects.trill` with the auxiliary `fret` (P − the track's open string − the track's capo) and the `speed` (§10). A trill on a percussion note, one whose `P` lies below the open string plus capo, or one whose speed property holds another number than the four above stays opaque, together with its speed property, and is replaced when a trill is set. An explicit `60` is kept, guarded by the trill, because the writer leaves the default out. The reader looks for the speed in every `XProperties` container of the note; two speed properties leave the trill opaque. The note's other `XProperty` entries are kept one by one, so the writer puts the speed and them into a single container, and it merges several containers into the first. Written notes follow GP's child order with `Trill` first, a kept `InstrumentArticulation` included; the one difference is `Vibrato` before `LetRing`, M1's order, which GP swaps on re-save. The writer refuses a trill whose auxiliary note sounds above MIDI 127 or sits on a percussion note (`TrillPitch`), and diagnose reports it (`trill_out_of_range`).

**Bends** have a fixed shape of four points:

1. origin: an offset and a value;
2. middle 1: offset 1 and the shared middle value;
3. middle 2: offset 2 and the shared middle value;
4. destination: an offset and a value.

Offsets run from 0 to 100 across the note. **GPIF counts bend values in hundredths of a tone: a value of 100 is a full tone.** GP exports 100 as `bend-alter 2` and 150 as `bend-alter 3`, so 25 is a quarter tone. Observed GPIF values are 0, 12, 25, 37.5, 50, 75, 100 and 150, and all observed offsets are whole numbers. The model counts in cents (hundredths of a semitone), so **model value = GPIF value × 2**: GPIF 25 is 50 cents and GPIF 37.5 is 75 cents. A GPIF value that does not become a whole number of cents, or an offset that is not a whole number, cannot be held by the model and stays opaque.

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
| `Score.document_extensions` | opaque root-level GPIF: `GPVersion`, `GPRevision`, `Encoding`, `BackingTrack`, `Assets`, `ScoreViews` and similar | Not tied to any element. `GPVersion 8.1.5` and the writer's own `GPRevision` are generated, so they are not kept. |
| `Score.container` | every archive entry in archive order: an entry for each file with its name and exact bytes, and one `Score` entry (name only) at the place where `Content/score.gpif` sat | At most one `Score` entry. File data is stored as base64 text in the JSON form. |
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
| `triplet_feel` | `TripletFeel`: `Triplet8th` → `triplet_eighth`, `Triplet16th` → `triplet_sixteenth`, `Dotted8th` → `dotted_eighth`, `Dotted16th` → `dotted_sixteenth`, `Scottish8th` → `scottish_eighth`, `Scottish16th` → `scottish_sixteenth`; absent → `none` | Written after `AlternateEndings` and before `DoubleBar`; no element for `none`. Any other form (`None`, an unknown name, padded text) is read for its known name and replayed whole under a guard while the feel is unchanged (§11.1). |
| `directions.targets` | `Directions/Target`, in order: `Segno`, `SegnoSegno`, `Coda`, `DoubleCoda`, `Fine` (§6.1) | snake_case in JSON (`segno_segno`). |
| `directions.jumps` | `Directions/Jump`, in order: the 14 names of §6.1 | snake_case in JSON (`da_segno_al_coda`). The writer emits the targets, then the jumps, after `DoubleBar` and before `Bars`; no element when both are empty. A `Directions` element in any other form (jumps before targets, an unknown name, an attribute, padded text, an empty element) is read for its known names and replayed whole while the directions are unchanged (§11.1). |
| `tempo_changes[].position` | `Automation[Type=Tempo][Bar=index]/Position` × bar length | The model counts every position (tempo changes and automations alike) in **whole notes from the start of its master bar** (`WholeNotes`); a tempo `Position` is a fraction of the bar, proven by the `tempo` golden's `export.mid` (§5). It is written with six significant digits, or more when six do not bring back its 1/3840 tick. |
| `tempo_changes[].bpm` | first number of `Value` | The number printed on the mark, not converted to quarter notes, kept as read (no rounding). `Tempo::exact()` gives it as an exact rational of at most six decimals (§5.2). `Visible` and `Text` → opaque. |
| `tempo_changes[].linear` | `Linear` = `true` | A ramp to the next tempo change of the score (§5.2). Written only when true; GP's `false` is kept under a guard. |
| `tempo_changes[].unit` | second number of `Value`: 1 eighth, 2 quarter, 3 dotted quarter, 4 half, 5 dotted half | Code 0 reads as quarter and is written as 2. Playback converts the mark to quarter notes. |

### Tracks and staves

| Model field | GPIF source | Notes |
|---|---|---|
| `Track.id` | `Track/@id` | id |
| `Track.name` | `Track/Name` | CDATA |
| `Track.short_name` | `Track/ShortName` | CDATA |
| `instrument.kind` | `InstrumentSet/Type`: `drumKit` → Percussion, anything else → Fretted | — |
| `instrument.set` | `InstrumentSet/Type`, one of the 12 types of §7 | An unknown type gives no set and stays opaque, guarded by the kind and set. A set of the other kind is refused (`InstrumentSet`). A track without a set is written with a type derived from its GM family (piano `acousticPiano`, organ `electricOrgan`, guitar `electricGuitar`, bass `electricBass`, strings `violin`, ensemble `voice`, brass `trumpet`, reed `saxophone`, drums `drumKit`; any other family `electricGuitar`), and reads back with that set. A file without `InstrumentSet` therefore re-saves. |
| `Track.drum_kit` | `InstrumentSet/Elements` of a `drumKit` track, §16 | Typed whenever its values can be read. Missing children take GP's defaults (§16): element `Name` and `SoundbankName` empty, `Type` `unknown`, no `Articulations` = no articulation; articulation `Name`, `TechniqueSymbol`, `OutputRSESound` empty, `StaffLine` 0, black noteheads, `TechniquePlacement` `outside`, no input key, `OutputMidiNumber` 0. Extra children, attributes, CDATA and non-shortest numbers are allowed. When the subtree is not exactly the writer's form (every corpus kit is: 10 kits, 540 elements, 950 articulations), the original `Elements` is also kept opaque, guarded by the kind and a digest of the typed kit, and written back unchanged while the kit is unchanged. An `Elements` whose values cannot be read (a child of `Elements` other than `Element`, of `Articulations` other than `Articulation`, a non-numeric or out-of-range number, an unknown placement, not exactly three noteheads) stays opaque untyped, guarded by the kind and the absence of a kit; a pitched track's `Elements` is kept the same way. Only the first `Elements` child is read. The set's `Name` and `LineCount` stay opaque and keep their place. A percussion track without a kit and without a kept `Elements` is written with `gm_drum_kit()` and reads back with it; a kit on a fretted track is refused (`DrumKitOnFretted`) and diagnosed (`drum_kit_on_fretted`). Kit text GPIF cannot hold is refused (`Character`). |
| `DrumKit.elements[e]` | `Element`: `name` ← `Name`, `kind` ← `Type`, `soundbank` ← `SoundbankName`, `count` = the number of its `Articulation` children | An element may hold no articulation. |
| `DrumKit.articulations[a]` | the `a`-th `Articulation` across all elements in document order: `name` ← `Name`, `staff_line` ← `StaffLine` (i8), `noteheads` ← the three names of `Noteheads`, `technique_placement` ← `TechniquePlacement` (`outside`, `inside`, `above`, `below`), `technique_symbol` ← `TechniqueSymbol`, `input_keys` ← `InputMidiNumbers` (space-separated, may be empty), `rse_sound` ← `OutputRSESound`, `output_key` ← `OutputMidiNumber` | Keys are 0..=127 and noteheads are non-empty names without whitespace (`DrumKit::new` refuses anything else). `DrumKit::key_of(a)` is the key GP plays. |
| `instrument.midi_program` | first `Sounds/Sound/MIDI/Program` | The initial program. |
| `midi.bank` | first `Sounds/Sound/MIDI/MSB`, `LSB` | The initial bank. |
| `sounds[k].name` | `Sounds/Sound[k]/Name` | CDATA. `Label` equal to the name is mapped; any other label is kept, guarded by the name. `Path`, `Role` and `RSE` stay opaque. |
| `sounds[k].program`, `.bank` | `Sounds/Sound[k]/MIDI/Program`, `MSB`, `LSB` | `sounds[0]` must match the initial program and bank (`FirstSound` refusal). |
| `sounds` empty | a single bare `Sound` holding only `MIDI` with `LSB`, `MSB` (only when not 0/0) and `Program`, in that order | The implicit sound of the track: written as `midi_program` and `midi.bank`, and read back as no sound. Any other `Sounds` content, a bare sound with explicit zero `LSB`/`MSB` included, is read as sounds, so nothing written is lost. Programs above 127 are a read error and a write refusal (`Program`). |
| `midi.port`, `.primary`, `.secondary`, `.per_string` | `MidiConnection/Port`, `PrimaryChannel`, `SecondaryChannel`, `ForeOneChannelPerString` | Channels 0 to 15. A missing element reads as GP's default: 0 for pitched tracks, 9 for drum kits, port 0, `false`. A `MidiConnection` that holds exactly that default (every corpus drum kit) is kept whole, guarded by the four values, so it is written back while they do not change; the writer generates the element only when the values differ from the default. |
| `midi.volume`, `midi.pan` | `RSE/ChannelStrip/Parameters` index 12 and 11 | Millionths, rounded half up when the decimal has more places, 500 000 when missing or not a decimal in 0..1. `Parameters` equal to GP's default list (§15) with these two values is mapped; any other list is kept, and the writer rewrites only the index whose value changed. |
| `automations[i]` with `Volume`, `Pan` | `RSE/ChannelStrip/Automations/Automation` of `Type` `DSPParam_12`, `DSPParam_11` | Typed only when the element is exactly what the writer generates: the six children in GP's order, `Bar` naming a master bar, `Position` in GP's 6-digit form on the 1/3840 grid (end of bar included) and `Value` in shortest decimal form. Anything else stays opaque, anchored to its bar. |
| `automations[i]` with `Sound` | `Track/Automations/Automation` of `Type` `Sound` | `Value` names a sound as `path;name;role`, from that sound's `Path` (empty when absent), name and `Role` (`User` when absent). Typed only when exactly one sound matches; the writer refuses an ambiguous one (`AmbiguousSound`), which diagnose reports for any sound automation naming a sound whose name another sound shares. A generated sound has no `Path`, so its automation reads `;name;User`, GP's link rule. |
| `automations[i].master_bar`, `.position`, `.linear`, `.visible` | `Bar`, `Position`, `Linear`, `Visible`. `Position` counts **quarter notes** for `Sound` (model position × 4) and is a **fraction of the bar** for `DSPParam_12` and `DSPParam_11` (model position ÷ bar length), §15 | `Bar` follows the master bar's id. Positions outside the 1/3840 grid or the bar are refused (`AutomationPosition`; diagnosed as `AutomationOffGrid` and `AutomationOutsideBar`), and a missing master bar is refused (`AutomationBar`). The model lists the channel-strip automations before the sound automations (`AutomationOrder` diagnoses another order, and the commands keep it); the writer keeps the order within each list. |
| `Score.sound_overrides` | the container entry of §1.2 | An override naming a missing track is refused (`SoundOverrideTrack`), and two overrides for one track are refused (`DuplicateSoundOverride`). |
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
| `Bar.voices` | the used slots of `Bar/Voices` | Slots holding `-1` are skipped. The writer puts each voice in its slot and fills the other slots with `-1`. A bar with no voice at all is GP's ordinary empty bar (59 in the corpus): the model holds it with an empty list, `diagnose` does not report it and the commands accept it. |
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
| `BeatEffects.tremolo_bar` | `Beat/Whammy`, seven attributes | Four points like a bend; values in cents (GPIF × 2, negative dives), offsets 0 to 100. Written right after `Rhythm` with six decimals (§8). |
| `BeatEffects.pop` | `Property Popped` | — |

### Notes

| Model field | GPIF source | Notes |
|---|---|---|
| `Note.id` | `Note/@id` | id; one model note per reference. |
| `NoteKind::Fretted.string` | (string count − 1) − `Property String` | — |
| `NoteKind::Fretted.fret` | `Property Fret` | — |
| `NoteKind::Percussion.articulation` | `InstrumentArticulation` on a drum track | An index into the track's kit (`Track::kit()`: the typed kit, or `gm_drum_kit()` when the track has none). GP silently refuses a drum track whose notes have no articulation (§16), so an index outside the kit, an empty kit included, is diagnosed (`articulation_outside_kit`) and refused by the writer (`ArticulationOutsideKit`), `set_drum_kit` and every restore. The writer cannot check a kept untyped kit. |
| percussion note `String` | `Properties/Property@String` on a drum note | Not in the model. The writer gives the notes of a drum beat distinct strings: note `i` of the beat gets string `i`, unless a kept string of the beat already uses it, then the smallest free one. The reader maps a `String` equal to the note's index in its beat and keeps any other value, guarded by the note's articulation (§16). A drum beat of more than 6 notes is refused (`TooManyDrumNotes`) and diagnosed (`too_many_drum_notes`); the commands refuse to restore one. |
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
| `bend` | `Bended` and the `Bend…` properties | Four points: origin, middle start, middle end and destination. The two middle points share one value. Offsets run from 0 to 100 as in GPIF. Values are in cents, from -1200 to 1200 (GPIF ±600): model value = GPIF value × 2 (§9). |
| `tap` | `Property Tapped` → Right, `Property LeftHandTapped` → Left | — |
| `left_hand_finger` | `LeftFingering` | `Open`, `P` Thumb, `I` Index, `M` Middle, `A` Ring, `C` Little. |
| `right_hand_finger` | `RightFingering` | `P` Thumb, `I` Index, `M` Middle, `A` Ring, `C` Little. |
| `trill.fret` | `Note/Trill` − the track's open string − the track's capo | Written as the note's first child (§9). |
| `trill.speed` | note `XProperty 688062467/Int`: 60 thirty-second, 120 sixteenth, 240 eighth, 480 quarter | Absent reads as thirty-second, which the writer leaves out. The model accepts only these four values (GP hangs on others). |

## 11. GPIF parts not modelled (opaque)

These stay as opaque payloads of their owner, or of the score:

- **Root level:** `GPVersion` (other than `8.1.5`), `GPRevision` (mandatory, see §12), `Encoding`, `BackingTrack`, `Assets`, `ScoreViews`.
- **Score:** `WordsAndMusic`, the page header and footer templates, and the system layout and zoom settings.
- **MasterTrack:**
  - `RSE`;
  - the `SyncPoint` automations;
  - the tempo automation's `Visible` and `Text`, a `Linear` that is not `true`, and the original text of its `Position`.
- **MasterBar:**
  - `Key/TransposeAs`, `Fermatas`;
  - a `TripletFeel` element that is not in the writer's form, replayed whole under a guard (§10);
  - `XProperties`;
  - a `Directions` element that is not in the writer's form, replayed whole under a guard (§10);
  - a repeat count on a start-only repeat.
- **Track:**
  - colour, layout, playing style and icon;
  - the `InstrumentSet` `Name` and `LineCount`, and an `Elements` the model cannot type (the type and a typed drum kit are mapped);
  - `NotationPatch`, `ForcedSound`;
  - the `RSE` content other than volume, pan and their automations: `ChannelStrip@version`, `ByPassedAutomations` and the other 14 parameters;
  - a sound's `Path`, `Role` and `RSE`, and a label that differs from its name;
  - `MidiConnection` children other than the four mapped ones, playback and engine state, `Lyrics`;
  - track automations other than typed sound automations, and `LetRingThroughout`.
- **Staff properties:**
  - `FretCount`, partial capo;
  - the tuning extras (`Flat`, `Instrument`, `Label`, `LabelVisible`) and `TuningFlat`;
  - the chord and diagram collections and working sets;
  - the tuning name.
- **Bar:** `SimileMark`, `XProperties`, and any clef other than `G2`, `F4` and `Neutral`.
- **Beat:**
  - the stem orientation elements;
  - `Variation`, `Wah`, `Golpe`, `Slashed`, `Hairpin`, `Legato`, `Arpeggio`, `Lyrics`, and a `Whammy` the model cannot hold;
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
  - A `Trill` the model cannot hold, an explicit default trill speed, and the note's other `XProperties`.

### 11.1 Opaque data after an edit

Each kept node or attribute is stored with the place it came from: a parent path below its owner and its index among the owner's children. Some kept data depends on a value that the model holds. That data carries a **guard**, the modelled value it was read under. On write, the writer compares each guard with the current value:

- When they match, the kept data is written back, and it replaces the element the writer would have generated in its place.
- When they differ, the kept data is dropped and the generated value is written.

A guard covers, for example, a note's derived pitches (string, fret, tuning, capo and transpose), `HopoDestination` (whether the note on the same string in the previous beat is a hammer-on origin; for the first beat of a bar, the previous beat is the last beat of the same voice slot in the previous bar), accent bits the model cannot hold (accent and staccato), the brush start time (the stroke), a `Directions` element not in the writer's form (the bar's directions) and the extras of a tempo automation (its position in the bar). The extras of a tempo automation follow the tempo change at the same position, so adding a tempo change at the start of a bar does not move them to another change.

Kept data that names a master bar by index is **anchored** to that master bar when it is read:

- the `Bar` of every track, `RSE` and master automation;
- the `BarIndex` of a sync point;
- the `Offset` of a lyrics line.

On write, the index is rewritten to the master bar's current position, so it follows inserted and removed bars. A sync point's `Bar` is recomputed in playback order (§5), and any difference GP's file had from that order is kept.

When an anchored master bar has been removed:

- an automation that names it is dropped;
- a sync point is dropped when its master bar or its pass no longer exists;
- a lyrics line starts at the next remaining master bar instead, or at the last one when none follows.

So the writer never writes an index to a bar that does not exist.

After the kept children of a master bar are inserted, the writer puts the children in GP's order of §6 (`Key`, `Time`, `Fermatas`, `Repeat`, `Section`, `AlternateEndings`, `TripletFeel`, `DoubleBar`, `Directions`, `Bars`, `XProperties`). A child with another name stays right after the child it followed. A bar read in that order is written unchanged; an edit that adds a child (directions, a marker, a double bar, a repeat or an ending) cannot push a kept `XProperties` before `Bars`. A source bar in another order is written in GP's order. Undoing the removal restores the bar's id, and the anchor finds it again. Data that is not in the plain integer form GP writes is kept as it is, without an anchor.

### 11.2 Opaque data the writer refuses

Opaque data can come from an `.opentab.json` file, which is untrusted input. The writer refuses it with `MalformedPayload`, and does not write XML that is not well formed, in these cases:

- **Names.** Every element name in a parent path, every kept element and attribute name, and the name of a kept attribute must be an XML `Name`. A `Property@…` or `XProperty@…` step holds an attribute value, so any text is allowed there. An ordinal after `#` must be a plain decimal number.
- **Depth.** A parent path is capped at 256 steps. The finished document may not nest deeper than 256 elements, which is the reader's limit.
- **Comments.** A kept comment may not contain `--` or end with `-`.
- **Anchors.** A bar anchor list must match the kept data.

## 12. Minimal GPIF

We wrote a one-track, one-bar score from scratch and removed parts one at a time. GP opened the following and exported it to MusicXML. The shape is schematic; the values are ours. We never showed that GP opens a score with no master bar or no track, and GP crashed on several other degenerate files, so the writer refuses both (`NoMasterBars` at `master_bars`, `NoTracks` at `tracks`).

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
- **`InstrumentSet` is required in practice.** A score that lacked both `GPRevision` and `InstrumentSet` crashed GP [m1]. We did not test a score that lacks only `InstrumentSet`, so a writer always emits one. The same score with a revision and a set opened [m3]. A set with only `Type` is enough for a stringed track; GP fills in the rest on save [i-iset-type-only]. It is **not** enough for a drum kit; see below.
- **`GPVersion` decides how sounds are read.** Without `<GPVersion>8.1.5</GPVersion>`, GP opens the file but ignores every track's `Sounds`. Each track then plays and re-saves as a "Drum Kit" sound with program 0. We wrote five different `Sound` shapes, richer `InstrumentSet` bodies and `Sound` automations, and all of them were ignored [t6-sound-full, t6-sound-automation]. With `GPVersion` alone added, the programs are kept and the MIDI export uses them [t6-sound-gpversion]. `Encoding` alone does not help [t6-sound-encoding]. A writer therefore always emits `GPVersion`.
- **`Sounds` is required once `GPVersion` is present.** A track with no `Sounds` element (program 0 left implicit) made GP's MIDI export crash [t6-mt-program0]. The same score with an explicit `Sounds/Sound/MIDI/Program` exported cleanly [t6-mt-program1]. A writer always emits `<Sounds><Sound><MIDI><Program>P</Program></MIDI></Sound></Sounds>`, even for program 0.
- **Every staff needs its tuning and capo.** A second staff without a `Tuning` property gets a tuning of six `C`s, and its notes come out at absurd pitches. A writer gives every staff the track's `CapoFret` and `Tuning`, unless the staff carries its own.
- **Chord diagram items need a `Diagram` child.**
  - An item with only `id` and `name` makes GP silently refuse the whole file, with no window and no message [t6-chord-chordonly].
  - So does an item whose `Diagram` has no attributes [t6-chord-bare-diagram]. The same file without chords opens [t6-chord-none].
  - `<Item id="i" name="N"><Diagram stringCount="S" fretCount="5"/></Item>` opens, and GP shows the chord name above the staff without a grid [t6-chord-empty-diagram]. `S` is the staff's string count.
  - On re-save GP adds its own `<Chord><KeyNote …/><BassNote …/></Chord>` to the item.
- **A drum kit needs GP's kit data.** A drum track whose `InstrumentSet` holds only `Type drumKit` cannot be used:
  - alone in a score, GP silently refuses the file. A dummy tuning, notes with `Midi`, `Fret` and `String`, and an explicit `PartConfiguration` did not change that [t6-drum-minimal, t6-drum-pc-midi];
  - after a guitar track, it opens, but renders as a TAB staff of `-2147483648` frets.

  A GP drum track passed through a reader and writer opens only when:
  - its `PartConfiguration` names the track [t6-drum-gp-nopc crashed, t6-drum-gp-pc opened];
  - it keeps its `InstrumentSet` body, including the articulation `Elements` [t6-drum-noiset, t6-drum-noelements crashed GP];
  - it keeps its `Sounds` and RSE data [t6-drum-nosound crashed GP].

  That data is GP's own content, and a writer may not copy it. A kit generated by our own code from the General MIDI key map is accepted instead (§16). With that kit an empty `PartConfiguration` was accepted, unlike the reduced GP kit above; a missing `Sounds` still crashed GP. OpenTab's writer therefore gives a kit-less percussion track its own General MIDI kit (§16) and always writes `Sounds`.
- **Everything else is optional for GP to open the file:**
  - `Encoding`;
  - the other `Score` fields;
  - tempo (GP defaults to 120);
  - colour, layout and playing style;
  - `Transpose`, `MidiConnection`, playback state;
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

On scores built from scratch we also saw the following. Each case is a file in `fixtures/gp/`, with its re-save committed as `goldens/gp/<file>/resave.gpif`.
- **Default tempo.** A score with no tempo at position 0 of the first bar gets a 120 quarter-note tempo there. Tempo automations gain `Linear false` and `Visible true`.
- **Characters outside the Basic Multilingual Plane** (emoji) become U+FFFD in titles and names. Accents and CJK text survive.
- **Empty voices.** A voice with no beats is removed from its bar.
- **Let ring on staccato notes.** `LetRing` is dropped from any note with the staccato accent bit. Palm mute, accent and vibrato do not affect it.
- **Harmonic frets.** A harmonic at a fret that is not a harmonic node, such as 2.5, is dropped together with its type and fret [t6-harm-natural25]. 3.2, 5, 7, 12 and 17 survive.
- **TransposedPitch spelling.** GP spells the written pitch by moving the concert spelling diatonically. In C sharp major with only an octave transposition, it keeps C sharp. For an instrument a tone below concert pitch, it writes B sharp. For a fourth plus an octave, it writes E sharp. A writer that picks the nearest enharmonic key instead differs only in spelling, never in pitch.
- **Ghost notes with an accent.** GP drops `AntiAccent` from any note that also has an `Accent` bit: accent, heavy accent or staccato. A ghost note without an accent keeps it [ghost-accent].
- **Container entries of our own** are dropped (§1.2).
- **Drum notes.** GP adds `ConcertPitch` and `TransposedPitch` (C, octave -1) and `Fret` and `Midi` (the articulation's `OutputMidiNumber`) to every drum note, keeps our `String`, and writes a `NotationPatch` of its own into the drum track [drums].
- **Tremolo bar.** GP continues a whammy from the previous beat's destination and shares beats that differ only in the whammy's middle value (§8) [tremolo-bar].
- **Revision.** A trill on a track with a capo raises `GPRevision required` to 12025 (§9) [trill].
- **Repeated direction names.** Only the last copy of each target or jump name in the score survives; earlier copies are removed from their bars (§5.1 rule 11) [directions-duplicates, directions-coda-free, directions-endings].
- **Channel strip.** A `ChannelStrip` written without its `version` attribute is accepted and comes back with GP's `version`. Its automations come back ordered by type: every `DSPParam_11` (pan) before every `DSPParam_12` (volume), each type keeping its own order; values, `Linear` and `Visible` are unchanged [mixer].
- **Sound of each beat.** In a track with more than one sound, GP adds `<Variation>k</Variation>` to every beat, where k is the 0-based index in `Sounds` of the sound active at that beat; beats under sound 0 get none. The sound active at a beat follows GP's reading of `Position` in quarter notes (§15) [sound-change].
- **The implicit sound.** A track whose only sound is a bare `Sound` with `MIDI/Program` comes back with that sound spelled out: empty `Name` and `Label`, GP's own `Path` and `Role`, `LSB` and `MSB`, and a `Sound` automation at bar 0, position 0, naming it. It also gains a `MidiConnection` with GP's default channels and a channel strip with GP's default parameters, which read as the defaults the model already holds (every fixture of §14).
- **Default view.** When `GPVersion` is present and the container carries no score views, GP's default view shows only the first track. The other tracks are present in the track list and in every export. Adding `ScoreViews` to the GPIF without the matching `Content/ScoreViews/*.gpsv` entries crashed GP [t6-views-one].

## 14. Committed goldens

The oracle harness saves every fixture through GP and exports it. The repository commits only output that holds no GP-authored assets:
- `resave.gpif`: only `Content/score.gpif`, taken from GP's re-save;
- `export.mid`;
- `export.musicxml`;
- `export.gp5`;
- `png/*.png`.

The full harvest stays local: the re-saved `.gp` container, `export.pdf` and `export.gpx`. The container and the GPX carry GP's default `BinaryStylesheet` and `Stylesheets/*.gpss`, and the PDF embeds font subsets. `cargo xtask oracle harvest` writes the full set into a directory outside the repository. `cargo xtask oracle publish --from <full> --out goldens/gp` copies the committed set and writes a manifest marked `"set": "published"`. `cargo xtask oracle check` then requires exactly that set.

Every fixture has a published golden. `drums.gp` was first refused by GP's GPX export (§16) and harvested once the writer gave drum notes distinct strings.

## 15. Track sound, MIDI channels and the mixer

```
<Track id="t">
  …
  <RSE>
    <ChannelStrip version="…">
      <Parameters>p0 p1 … p15</Parameters>
      <ByPassedAutomations>DSPParam_12</ByPassedAutomations>     optional
      <Automations>
        <Automation><Type>DSPParam_12</Type><Linear>false</Linear><Bar>B</Bar><Position>P</Position><Visible>true</Visible><Value>V</Value></Automation>
      </Automations>                                             optional
    </ChannelStrip>
  </RSE>
  <Sounds>
    <Sound><Name/><Label/><Path/><Role/><MIDI><LSB>L</LSB><MSB>M</MSB><Program>P</Program></MIDI><RSE/></Sound>
  </Sounds>
  <MidiConnection>
    <Port>0</Port><PrimaryChannel>C</PrimaryChannel><SecondaryChannel>C</SecondaryChannel>
    <ForeOneChannelPerString>false</ForeOneChannelPerString>
  </MidiConnection>
  …
  <Automations>
    <Automation><Type>Sound</Type><Linear>false</Linear><Bar>B</Bar><Position>P</Position><Visible>true</Visible>
      <Value><![CDATA[path;name;role]]></Value></Automation>
  </Automations>
</Track>
```

**MIDI connection.** All 64 tracks have one, with exactly the four children shown. The element name is spelled `ForeOneChannelPerString` by GP, and it is `false` everywhere. `Port` is 0 everywhere. `PrimaryChannel` and `SecondaryChannel` are 0-based MIDI channels:
- drum kits use 9 for both (10 tracks);
- every other track has the next free channel above its primary channel as secondary, skipping 9 (54 tracks: 0/1, 2/3, 4/5, 6/7, 8/10, 11/12, 13/14).

A separate empty `UseOneChannelPerString` element appears in 45 tracks. Without a `MidiConnection`, GP gives a pitched track channel 0 for both and a drum kit channel 9 [m3-mx01, m3-dr12].

**Sounds.** 66 sounds in 64 tracks; two tracks have two. Every sound has `Name`, `Label`, `Path`, `Role`, `MIDI` and `RSE`. `MIDI/LSB` and `MIDI/MSB` are the bank select bytes and are 0 in all 66; GP keeps other values we wrote (MSB 1, LSB 3) on re-save [m3-mx03]. `Role` is `Factory` (40) or `User` (26). On re-save GP replaces `Path` and `Role` of a sound we wrote with its own library entry for the program, and keeps `Name` [m3-mx04].

**Sound automations.** These are the track's `Automations`. All 67 in the corpus have `Type` `Sound`, `Linear` false and `Visible` true. Every track starts with one at bar 0, position 0, that names its first sound (64). 3 more switch sounds later; one of them sits at position 1, which GP reads as one quarter note into its bar (below). `Value` is one CDATA text, `path;name;role`, and in all 67 it names a sound of the same track by those three fields. How GP resolves our values:
- A sound written without `Path`, referenced as `;name;User`, is found. GP then rewrites both the sound and the automation with its own path, and the role became `Factory` in [m3-mx11] but stayed `User` in [sound-change].
- A value whose path part is a path of our own, or even GP's own library path, was re-pointed to the track's first sound [m3-mx04, m3-mx10, m3-mx12, m3-mx13]. A writer should therefore leave `Path` out of sounds it generates and reference them as `;name;User`.

**A sound automation's `Position` counts quarter notes from the start of its bar**, unlike a tempo automation's, which is a fraction of the bar (§5). The new sound applies from the first beat that starts at or after that offset. Evidence: with four quarter-note beats in a bar, `Position` 0.25, 0.5, 0.75 and 1 all switched the sound from the second beat, 2 from the third and 3 from the fourth, judged by the `Variation` GP writes on each beat (§13) [m3-t4-s-pos-0.25 … 3]. The same bar in 2/2 gave the same result for 1 and 2, so the unit is the quarter note and not the time signature's beat [m3-t4-s-pos-1-cut, m3-t4-s-pos-2-cut]. GP kept every value, 2 and 3 included, on re-save. A second meter agrees: in a 7/8 bar of eighth notes, `Position` 0.75 switched from the third eighth (one quarter in) and 1.5 from the fourth, where a bar fraction would have given the seventh eighth and nothing [m3-t7-o78]. The regenerated [sound-change] writes 2 in a 4/4 bar, and GP switches from the third beat. OpenTab reads and writes it in quarter notes (§10); the `sound-change` golden's `Variation` marks are checked against that reading by `oracle_fidelity`.

**Volume and pan `Position`: a fraction of the bar, observed.** GP's GP5 export writes each strip automation as a mix-table change (volume or balance) on a beat, so its beat shows where GP places the point. We wrote 16 volume and pan points at bar fractions 0.05 to 0.99 into scores of 32nd-note beats in 4/4, 5/4 and 7/8 [m3-t7-s44, m3-t7-s54, m3-t7-s78]. All 48 landed on the last beat that starts at or before `Position` × the bar's length; read as quarter notes, they would all sit in the bar's first quarter. In [mixer] (4/4, quarter beats) the pan at 0.5 lands on the third beat. One detail is unexplained: a six-digit value just above a beat boundary, such as 0.714286 for 5/7 of a 7/8 bar, lands one beat early, like one just below it; exact decimals on a boundary land on that beat [m3-t7-s78e, m3-t7-e78e, m3-t7-o78]. The corpus agrees. First, `0.785714` and `0.857143` sit in a 7/8 bar and are 11/14 and 6/7: the bar's numerator 7 is in their denominators, so GP divided by the 7/8 bar; read that way they are the beats at 11/16 and 3/4 of a whole note, while as quarter notes they fall on no beat. Second, of the 8 strip automations inside their bar, 4 land on a beat as bar fractions and only 2 as quarter notes (0.25 and 0.75 in 4/4 bars fit both); the other four (0.00625, 0.0416667, 0.625, 0.99375) fit neither, as GP's automation editor lets a point be dragged anywhere. The corpus test `corpus_automations_inside_a_bar_land_on_beats_in_the_units_we_read` pins these counts, together with tempo (4 hits as a bar fraction, 1 as quarter notes) and sound (1 and 0). The unit per automation kind is one exhaustive match in the GPIF code, so a new kind cannot inherit a unit by default.

**Mixer.** Every track has `RSE/ChannelStrip/Parameters` with 16 numbers from 0 to 1. **Index 12 is the volume and index 11 the pan.** Evidence: GP's GP5 export writes these two into the GP5 channel table [m3-mx01]:
- volume 0.2, 0.5 and 0.8 became 1, 4 and 16 there, so the curve is not linear;
- pan 0.1, 0.5 and 0.9 became 1, 8 and 14.

GP's MIDI and MusicXML exports ignore both: MusicXML says volume 80 and pan 0 whatever the values are [m3-mx01]. In the corpus, volume runs from 0.47 to 0.8125 and pan from 0 to 1, with 0.5 as the centre (45 of 64). GP writes 0.5 for both when a file has no channel strip: every committed `resave.gpif` of §14 holds `0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0 0.5 0.5 0.5 0.5 0.5 0.5`, GP's default list, which OpenTab's writer also uses around a volume or pan of its own. Every value is a decimal of at most 4 places in its shortest form.

**Volume and pan automations** live in the channel strip, not in the track's `Automations`:
- `DSPParam_12` is volume (79 automations) and `DSPParam_11` is pan (65), in 24 files;
- each has the six children of a tempo automation, and `Value` is a decimal from 0 to 1 of at most 6 places in its shortest form;
- 1 is linear, 18 are not visible, and 8 sit inside their bar at a 6-significant-digit `Position` such as 0.785714;
- `ByPassedAutomations` (50 tracks) lists automation types separated by spaces: `DSPParam_12` 37, both 12, `DSPParam_11` 1. Its effect on playback was not tested.

GP keeps the automations we wrote, linear or not, on re-save [m3-mx05, m3-mx09].

## 16. Drum kits

```
<InstrumentSet>
  <Name>…</Name><Type>drumKit</Type><LineCount>5</LineCount>
  <Elements>
    <Element>
      <Name>…</Name><Type>ELEMENT</Type><SoundbankName/>
      <Articulations>
        <Articulation>
          <Name>…</Name><StaffLine>S</StaffLine>
          <Noteheads>GLYPH GLYPH GLYPH</Noteheads>
          <TechniquePlacement>outside|inside|above|below</TechniquePlacement><TechniqueSymbol/>
          <InputMidiNumbers>K</InputMidiNumbers><OutputRSESound/><OutputMidiNumber>K</OutputMidiNumber>
        </Articulation>
      </Articulations>
    </Element>
  </Elements>
</InstrumentSet>
```

The corpus has 10 drum kits. Each has 54 elements and 95 articulations, with every child shown above. The element `Type` values name the drum instrument; the corpus uses 32 of them (for example `kickDrum`, `snare`, `hiHat`, `tom`, `ride`, `crash`); a missing or unknown type is re-saved as `unknown` [m3-dr16]. `Noteheads` names three SMuFL glyphs: the heads for black, half and whole notes.

**A drum note** uses `InstrumentArticulation` as an index into the kit's articulations, counted across all elements in document order. In the corpus (2 327 drum notes) its `Fret` and `Midi` properties both equal the articulation's `InputMidiNumbers`, and its `String` is a number from 0 to 5.

**A kit of our own opens.** We generated a General MIDI kit in our own words: 37 elements and 47 articulations, one per GM percussion key 35 to 81, with our names and our element grouping. GP opened it with no notice, alone or after a guitar track, and with an empty `PartConfiguration` [m3-dr01, m3-dr02, m3-dr03, m3-dr04]. The MIDI export played each note on channel 9 at its articulation's `OutputMidiNumber`.

**What GP needs**, found by removing parts one at a time:
- **`Elements` with articulations.** A set with only `Type drumKit` and notes is silently refused, as in §12 [m3-dr24]. With no notes it opens [m3-dr14].
- **`OutputMidiNumber`** is the key GP plays. Without it every note plays key 0 [m3-dr19]. `InputMidiNumbers` is not used for playback [m3-dr20].
- **`Sounds`.** A drum track without `Sounds` crashed GP while opening [m3-dr11]. `<Sounds><Sound><MIDI><Program>0</Program></MIDI></Sound></Sounds>` is enough.
- **Defaults.** Every other child may be left out, and GP fills it in on re-save [m3-dr05 … m3-dr10, m3-dr13, m3-dr15 … m3-dr18, m3-dr22]:
  - `Name` empty, `Type` `unknown`, `SoundbankName` empty;
  - `StaffLine` 0, black noteheads (`noteheadBlack noteheadHalf noteheadWhole`), `TechniquePlacement` `outside`;
  - empty `TechniqueSymbol`, `InputMidiNumbers` and `OutputRSESound`;
  - set `Name`, `LineCount` 5, the `MidiConnection` (channel 9) and the note properties.
- **Tuning and note properties.** A drum track whose staff has no `Tuning` and whose notes have no `String` and `Fret` opens. But GP's GP5 and GPX exports then stop at a "Notes outside range" box [m3-dr23, m3-dr25, m3-dr26]. With a `0 0 0 0 0 0` tuning, or with `Fret`, `Midi` and `String` on the notes, all exports run [m3-dr09, m3-dr13]. A writer should give drum notes `Fret` and `Midi` equal to the input key and a `String`, and the staff a six-zero tuning, as GP's own files do.
- **One string per note of a beat.** The six-zero tuning is not enough when a beat holds several drum notes. GP gives a note without `String` string 0, and its GPX export drops notes that share a string within a beat, behind the same "Notes outside range" box; GP5 assigns strings itself and keeps all of them. The `drums` fixture (four notes per beat, no `String`) failed this way, and so did GP's own re-save of it, which wrote `String` 0 on all 47 notes. The same file with `String` 0 to 3 on the notes of each beat exported cleanly, with or without `Fret` and `Midi` [drums, m3-t7-d-gp-resave, m3-t7-d-string, m3-t7-d-full]. In the corpus no drum beat repeats a string (4 877 beats, at most 6 notes each). A writer should give the notes of a drum beat distinct strings 0 to 5; a beat of more than six drum notes cannot be exported to GPX. OpenTab's writer does so (§10), and refuses a drum beat of more than six notes. The regenerated `drums` fixture (strings 0 to 3 in each beat) passed every export, GPX included; GP kept our strings on re-save [drums].

**OpenTab's General MIDI kit.** `gm_drum_kit()` is built by our code, not copied from any GP file: one element per key of the General MIDI Level 1 percussion map (keys 35 to 81, 47 elements of one articulation each), in key order, so articulation `a` plays key `35 + a`. Each element and its articulation carry the GM name of the key. `StaffLine` and the noteheads come from OpenTab's own percussion table (the one layout draws with): bass drums on the bottom space (7), snares on the second space (3), toms on 1, 2 and 5, hi-hats above the staff (-1) and the pedal hi-hat below it (9), cymbals on 0 to -4, every other key on the middle line (4); cross heads (`noteheadXBlack noteheadXHalf noteheadXWhole`) for cymbals, hi-hats, side stick, tambourine and cowbell, black heads (`noteheadBlack noteheadHalf noteheadWhole`) otherwise. Element `Type` is one of the documented names `kickDrum`, `snare`, `hiHat`, `tom`, `ride`, `crash`, or `unknown`. `InputMidiNumbers` and `OutputMidiNumber` both hold the key, `TechniquePlacement` is `outside`, and `SoundbankName`, `TechniqueSymbol` and `OutputRSESound` are written empty, the form GP itself re-saves when they are left out. The generated `drums` fixture plays every key of this kit. GP opened it with no notice, and its MIDI export played all 47 notes on channel 9 at keys 35 to 81, each at its fixture onset and length, so at the keys the kit declares [drums]. Its golden was harvested once the writer gave each note of a beat its own string (above).

**Drum tuning.** A percussion track whose model tuning is empty is written with the six-zero tuning GP's own drum tracks carry (see "Tuning and note properties" above), and reads back with it.

**Layout** draws a drum note from its articulation's `OutputMidiNumber` through OpenTab's own percussion table; the kit's `StaffLine` and `Noteheads` are kept but not used for drawing yet.

**StaffLine** places the articulation on the five-line staff. 0 is the top line, and each step of 1 moves down by one line or space, so 8 is the bottom line; negative values sit above the staff. We read this from GP's PNG of a kit whose articulations had staff lines -2 to 10 [m3-dr21]. It is a visual reading, not a measurement.

## 17. GP's MIDI export

What GP's `export.mid` does with the data above. The comparison profile that copies GP's export relies on these points:
- **Format.** SMF format 1 at 480 ticks per quarter. Track 0 carries the tempo and time signature, then one MIDI track per score track. At tick 0, after the first tempo and time signature, track 0 holds the score title as a track-name event (0x03) and then the copyright (0x02); either is left out when empty [metadata, from-scratch, every golden]. OpenTab writes both in both profiles.
- **Channels.** Each track plays on its `PrimaryChannel`. Bent notes stay on that channel; the secondary channel is not used [m3-mx02]. Drum kits play on channel 9 [m3-dr01].
- **Program.** The first sound's program is set at tick 0. Sound automations do not produce program changes, even when GP resolved them [m3-mx04, m3-mx11].
- **Bank, volume and pan are not exported**: no bank select, no volume and no pan controllers, whether they come from the channel strip or its automations [m3-mx01, m3-mx03, m3-mx05, m3-mx09]. Each track sets a pitch-bend range of 6 semitones.
- **Tempo.** One event per tempo automation, at its bar fraction; linear ramps are not expanded, so each tempo holds until the next automation. The value is `floor(60 000 000 / (BPM × f))` µs, with a dotted quarter counted as 1.25 quarters (§5, §5.2) [tempo, tempo-ramp]. At every backward jump (the played bar's source index is lower than the previous one) GP restates the tempo in force at the target bar's source position, whether or not it changes; it writes nothing at a forward jump such as To Coda, nor at a one-bar repeat of itself, where the held tempo continues [m3-gp9-04, tempo-forward-jump, tempo-self-repeats].
- **Whammy** is not exported: the notes of the [tremolo-bar] golden play at their plain pitches with no pitch bend [m3-mx07, tremolo-bar].
- **Drum kits** play each note at its articulation's `OutputMidiNumber` on channel 9 [m3-dr01, drums].
- **Velocity** comes from the beat's dynamic only: ppp 25, pp 38, mf 76, ff 102, fff 114 in the `ghost-accent` fixture. Ghost notes, accents and heavy accents get the same velocity as plain notes, alone or in chords.
- **Staccato** halves a note, but not a note that also has an accent: in the `ghost-accent` fixture, accent + staccato plays its full length while ghost + staccato (re-saved as staccato only) plays half.
- **Trill** alternates principal and auxiliary notes every `T` ticks, starting with the principal, `length / T` notes per trilled note (§9). Defect: the note-off after each auxiliary note names the principal's key, so the file never releases the auxiliary key [m3-tr12, m3-tr13, trill].
- **Directions and repeats** are expanded in the order of §5.1.
- **Unisons on one channel.** When voices, staves or tracks share a channel, GP writes two notes of the same key on that channel that overlap in time [multi-track, unisons]; a synth releases both at the first note-off. A same-tick unison gets two note-ons in voice order, each at its own beat's velocity (89 and 51 for f and p), and two note-offs at each note's own end; a later strike of a sounding key gets its own note-on with no cut; a bend on one of the two bends the whole channel [unisons]. Our export ends the earlier note at the re-strike and keeps one note for a same-tick unison, at the louder velocity and reported as `MergedUnison`, in both profiles (under `musical` a note with a bend, vibrato, slide or tremolo bar sits on a channel of its own when one is free, and wins the merge when the pool is exhausted); the comparison applies the same cut to GP's notes (allowance `restrike-cut`).
- **Vibrato on a tie destination** starts at the onset of the tied note, not at the destination, within a bar and across a bar line [unisons, z-corpus-edited].
- **Triplet feel** swings the bar [triplet-feel, first harvested as m3-t11-swing: 7 feels × 16 rhythms, 112 bars]. An eighth feel works on pairs of eighths inside each quarter, a sixteenth feel on pairs of sixteenths inside each eighth, counted from the start of the bar (6/8 too). Only a note whose length is a whole number of the feel's value (eighths, or sixteenths) moves: its start and its end are mapped through the pair, the first half of the pair stretched or squeezed to the split and the second half filling the rest. The split is 2/3 for `Triplet`, 3/4 for `Dotted` and 1/4 for `Scottish`. Shorter notes, dotted values that are not a whole number of the feel's value (a dotted eighth under an eighth feel) and tuplets keep their straight times, even when a moved note now overlaps them. We assume a grace note follows its moved principal (inference: the experiment holds no grace beats; the corpus-derived `z-corpus-edited` export shows an on-beat grace at the swung off-beat). Examples at 480 ticks: `Triplet8th` eighths 320/160, `Dotted8th` 360/120, `Scottish8th` 120/360, `Triplet16th` sixteenths 160/80; a quarter on the off-beat under `Triplet8th` starts at 320 and keeps 480 ticks; an eighth on the second sixteenth of a beat plays from 160 to 400.
