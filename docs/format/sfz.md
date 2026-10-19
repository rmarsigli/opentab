# The SFZ subset OpenTab implements

This is a clean-room description. It is derived only from two sources: the public SFZ format documentation (sfzformat.com), and a survey of the `.sfz` text files of 21 free guitar, bass, drum and general-MIDI sound libraries (1,304 files, about 62,000 regions), which is summarised in the M3 survey ledger. No player or loader source code was consulted, and no text of any library or site is reproduced here. Where the public documentation leaves a behaviour open, this document states the rule OpenTab uses and marks it **our rule**.

Words used in this document:

- **Bank root** is the folder the user imported or installed an SFZ bank into. Every file OpenTab reads for a bank lives under it.
- **Instrument** is one `.sfz` file that is not included by another file (a **root file**).
- **Region** is one `<region>` after inheritance, the unit that owns one sample and decides when it sounds.
- **Subset** means the opcodes in section 5. Everything else is **ignored and listed** (section 8).
- **Note-on event** is one MIDI note-on arriving at the SFZ backend for one channel.

## 1. Scope

The SFZ backend plays notes from OpenTab's MIDI event stream. It has no live controller input beyond the controllers that stream carries. That shapes the subset: opcodes that modulate by a controller a score never moves (filters, LFOs, envelopes driven by CC, effects) are out of scope, and opcodes that select between variants (keyswitches, controller-gated regions, release triggers, round robin) are in scope only to the extent needed to pick the one default variant deterministically.

The backend never executes anything from a bank. A bank is data: `.sfz` text and audio files.

## 2. Files and text

- Encoding: UTF-8. A leading byte order mark is skipped. A file that is not valid UTF-8 is a load error for that instrument. All 1,304 surveyed files were valid UTF-8.
- Line ends: LF, CR LF and lone CR all end a line. Libraries written on Windows use CR LF.
- Size limits (our rule, so a hostile bank cannot exhaust memory): section 11 lists every limit. The text ones are 4 MiB per file, 16 MiB of text per instrument after includes and variables are expanded, 100,000 regions per instrument, include depth 8, 4,096 includes per instrument, 16,384 `#define` lines and 1 MiB of definitions. The largest surveyed file is 210 KB, and the largest expanded instrument is 8,902 regions and 0.95 MB.
- Banks are untrusted user imports, so a limit being hit refuses the whole bank (it fails closed), with a message that names the limit. A file that is not valid UTF-8 is not a limit: that instrument is skipped and reported, and the others load.

## 3. Syntax

### 3.1 Comments

`//` starts a comment that runs to the end of the line. `/*` starts a comment that runs to the next `*/`, across lines. Comments are removed before anything else, including before directives. A `//` inside a quoted `#include` path is not expected and is not special-cased.

### 3.2 Directives

Directives are processed in file order, before tokenising, as a text substitution.

- `#define $NAME value`: the name is `$` followed by letters, digits and underscores; the value is the rest of the line after one run of white space, trimmed, and taken literally: variables inside a value are not replaced (our rule, which also stops definitions from growing each other). A definition applies from that line to the end of the instrument, including inside files included later and after the include that made it. Redefining a name replaces it from that point on. A malformed definition is ignored and listed. Survey: 1,449 definitions in 4 libraries, every one of the form `#define $NAME 123` with a number.
- Variable use: every `$NAME` in later text is replaced by its value. Substitution is textual and can occur anywhere, including inside an opcode name (`amplitude_oncc$VOL` becomes `amplitude_oncc105`) and inside a path. The longest defined name matching at a position wins, so `$KEY` does not rewrite `$KEY_KICK`. Substitution is applied once; the result is not scanned again.
- A `$NAME` that was never defined stays as written. An opcode whose value or name still contains `$` after substitution is ignored and listed as "unresolved variable"; for `sample` that drops the region. One surveyed library (a template file whose `default_path` uses an undefined variable) shows why this must not be fatal.
- `#include "path"`: replaced by the content of that file, itself preprocessed. Several includes may share a line (`#include "a" #include "b"` occurs, 16 times in one library), and text around them on the line is kept. Variables in the path are replaced; a path that still holds `$` is listed as "unresolved variable". Path rules are in section 6.4. The extension is not checked: one library includes `.txt` files that hold regions (116 includes).
- A missing or unreadable include, or one that leaves the bank, is a warning, not an error: the include contributes nothing and is listed. Surveyed libraries contain dozens of includes that resolve to nothing, usually fragment files.
- An include cycle (a file that includes itself through any chain), nesting deeper than 8, more than 4,096 includes in one instrument, or an included file larger than 4 MiB refuses the bank (section 11). No surveyed library comes near these.
- Any other line starting with `#` is ignored and listed.

### 3.3 Headers and opcodes

After directives, the text is a sequence of tokens separated by white space and lines. A token is either a header, `<name>`, or an opcode assignment, `name=value`. Headers and assignments may share a line (`<region> lokey=36 hikey=36 sample=a.wav` is common).

- Opcode names are matched case-insensitively, by ASCII case folding. SFZ names are lower case, but one surveyed library writes `Sample=`.
- A value starts after `=` and runs up to, but not including, the next white space that is followed by `name=` or by `<`, or up to the end of the line. White space inside a value is kept, so `sample=Kick Drum 1.wav` is one value. The value is trimmed.
- Header names are matched case-insensitively. Known headers: `control`, `global`, `master`, `group`, `region`. Recognised and ignored, with every opcode that follows them: `curve`, `effect`, `midi`, `sample`. Any other header is also ignored. Opcodes under an ignored header never leak into the previous known header: they are skipped until the next known header appears. This matters: `<curve>` bodies use opcodes named `v000`..`v127` and `<effect>` bodies carry plugin parameters.
- An opcode before any header, or an assignment with an empty name, is ignored and listed.
- Numbers use a decimal point and optional sign (`0.250`, `-3.500`, `100`); an exponent is also read. Infinite and not-a-number values are invalid. An integer opcode given a fraction is rounded to the nearest integer. An opcode whose value does not parse for its type is ignored and listed as "invalid value". A value outside the range in section 5 is clamped and counted as "clamped".

## 4. Header inheritance

Headers form a fixed hierarchy:

`control` (its own scope), and `global` > `master` > `group` > `region`.

- Opening `<global>` clears the global, master and group scopes. Opening `<master>` clears master and group. Opening `<group>` clears group. Opening `<region>` starts a new region and leaves the others as they are.
- `master` and `group` are optional levels, and `global` may be absent. Survey: `master` appears in 7 of 20 libraries (1,225 times) and `global` in 17 (144 times).
- A region's value for an opcode is the one set at the most specific scope that sets it: region, then group, then master, then global. Inheritance replaces; it never adds. For example, `volume=-3` in a group and `volume=2` in the region give `2`.
- Inside one scope the last assignment of an opcode wins.
- `<control>` opcodes (`default_path`, `set_ccN`) apply to the whole instrument from where they appear: `default_path` is joined to a region's `sample` when the region closes, so a later `<control>` changes it only for the regions after it. A later `set_ccN` replaces the same controller's value and leaves the others. Other opcodes under `<control>` are listed.
- Regions with `sample` missing, or with `sample` empty, are dropped and listed.

## 5. The opcode subset

Ranges are the accepted ones; out-of-range values are clamped. Defaults apply when no scope sets the opcode. "Survey" is the number of the 20 libraries with parseable regions that use the opcode.

### 5.1 Sample and mapping

| Opcode | Type, range | Default | Meaning | Survey |
|---|---|---|---|---|
| `sample` | path | none | Audio file of the region; rules in section 6. Names starting with `*` are generators (`*sine`, `*silence`): the region is dropped and listed | 20 |
| `default_path` | text, `control` only | empty | Prepended, verbatim, to every `sample` value; it needs its trailing separator | 4 |
| `key` | note 0..127 | none | Sets `lokey`, `hikey` and `pitch_keycenter` to the same note | 13 |
| `lokey`, `hikey` | note 0..127 | 0, 127 | Inclusive key range. `lokey` greater than `hikey` is a valid empty range: the region never sounds | 16 |
| `pitch_keycenter` | note 0..127 | 60 | The key at which the sample plays at its recorded pitch | 16 |
| `lovel`, `hivel` | integer 0..127 | 0, 127 | Inclusive velocity range. `lovel` greater than `hivel` is empty (a surveyed library uses `lovel=101 hivel=100` to disable regions) | 15 |

Note values accept numbers and note names (section 7). A number outside 0..127 is clamped and listed; a name outside that range is invalid.

### 5.2 Pitch

| Opcode | Type, range | Default | Meaning | Survey |
|---|---|---|---|---|
| `tune` | cents, -9600..9600 | 0 | Fine tuning. The public range is -100..100, but surveyed libraries use -1200 | 4 |
| `transpose` | semitones, -127..127 | 0 | Whole-semitone shift of the sounding pitch | 4 |
| `pitch_keytrack` | cents per key, -1200..1200 | 100 | How much pitch follows the key. 0 keeps the sample at its recorded pitch | 1 |

Playback rate is the sample's rate over the output rate, times two to the power of (key minus `pitch_keycenter`, times `pitch_keytrack`, plus 100 times `transpose`, plus `tune`, plus the channel's pitch bend, all in cents, over 1200). Pitch bend comes from the MIDI stream in the range the stream sets by RPN 0 (M3-D6). `bend_up`, `bend_down` and `bend_step` are ignored and listed, because a bank's own bend range would contradict the stream.

### 5.3 Level and pan

| Opcode | Type, range | Default | Meaning | Survey |
|---|---|---|---|---|
| `volume` | dB, -144..6 | 0 | Gain in decibels | 8 |
| `amplitude` | percent 0..100 | 100 | Linear gain, multiplied with `volume` | 2 |
| `pan` | -100..100 | 0 | -100 is full left, 100 full right. Mono samples are placed; stereo samples are balanced. Centre is unity gain per channel. The law is constant power; the exact curve is our rule | 5 |
| `amp_veltrack` | percent -100..100 | 100 | How much velocity scales the gain. 0 plays every velocity at the same level | 5 |
| `amp_velcurve_N` | gain 0..1, N 0..127 | none | Gain at velocity N. When any point is set, the velocity gain is linear interpolation through the points, with an implied point at velocity 0 of gain 0 and, past the last point, the last gain | 10 |

Velocity gain, our rule: with `v` the velocity over 127 and `t` the veltrack over 100, gain is `(1 - t) + t * curve(v)`, never below 0, where `curve(v)` is `v` squared by default, or the `amp_velcurve_N` interpolation when points exist. A velocity of 0 is a note-off, never a note-on. Inheritance merges curve points by velocity: a region keeps its own points and takes the parent's points at other velocities.

Channel level, our rule, the same law as the built-in synthesizer: controller 7 scales both sides by the square of its value over 127, and controller 10 is a balance (64 is centre at unity, 0 silences the right side, 127 the left). Region gain, velocity gain and channel level multiply. The backend adds no other gain stage; its output is clamped to the mixer's track range and any non-finite value becomes silence.

### 5.4 Amplitude envelope

A linear envelope, our rule: attack rises from 0 to 1, hold stays at 1, decay falls to the sustain level, sustain holds until note-off, release falls to 0 from wherever the level was. Shape opcodes (`ampeg_*_shape`) are ignored and listed.

| Opcode | Type, range | Default | Survey |
|---|---|---|---|
| `ampeg_attack` | seconds 0..100 | 0 | 8 |
| `ampeg_hold` | seconds 0..100 | 0 | 6 |
| `ampeg_decay` | seconds 0..100 | 0 | 8 |
| `ampeg_sustain` | percent 0..100 | 100 | 8 |
| `ampeg_release` | seconds 0..100 | 0.001 | 19 |

`ampeg_delay` and `ampeg_start` are ignored and listed. A voice that reaches the end of its sample without looping ends then, whatever the envelope. The backend applies a fade of at least one millisecond on every voice end to avoid clicks, whatever `ampeg_release` says.

### 5.5 Looping, offsets and one-shots

| Opcode | Type, range | Default | Meaning | Survey |
|---|---|---|---|---|
| `loop_mode` (also `loopmode`) | `no_loop`, `one_shot`, `loop_continuous`, `loop_sustain` | `no_loop` | `no_loop` plays to the end or until note-off plus release. `one_shot` plays to the end and ignores note-off (drums). `loop_continuous` loops until the voice ends. `loop_sustain` loops until note-off, then plays on without looping through the release | 11 |
| `loop_start`, `loop_end` (also `loopstart`, `loopend`) | sample frames | whole sample | Inclusive loop points. Without them, a looping region loops the whole sample. Loop points stored inside audio files are not read | 1 |
| `offset` | frames 0.. | 0 | First frame played | 3 |
| `end` | frames, or -1 | last frame | Last frame played. -1 disables the region (it is dropped and listed) | 3 |

Loops have no crossfade. A loop whose points fall outside the sample, or after `end`, is clamped to them; a loop shorter than one frame does not loop. Playback starts at `offset`, even when that is inside the loop.

Resampling, our rule: the playback position advances by the playback rate each output frame, and each output value is the linear interpolation of the two neighbouring sample frames; inside a loop the neighbour of the last loop frame is the first one.

### 5.6 Choke groups

| Opcode | Type, range | Default | Meaning | Survey |
|---|---|---|---|---|
| `group` | integer 0..4294967295 | 0 | Group number of the region; 0 is no group. A value may be a `$` variable | 9 |
| `off_by` | integer 0..4294967295 | 0 | When a voice starts whose `group` equals this number, the voices of this region are turned off; 0 means never | 9 |
| `off_mode` | `fast`, `normal`, `time` | `fast` | How a choked voice ends: `fast` uses a 6 ms fade, `normal` uses the voice's own `ampeg_release`, `time` uses `off_time`. A choked `one_shot` voice ends too | 5 |
| `off_time` | seconds 0..100 | 0.006 | Fade time for `off_mode=time` | 2 |

Rules, our rule: a choke applies to voices of the same backend instance, on any channel. Voices started by the same note-on event never choke each other, so a hi-hat with several layers in one group does not silence itself. Open and closed hi-hat, cymbal chokes and mute groups all use this.

### 5.7 Round robin

| Opcode | Type, range | Default | Meaning | Survey |
|---|---|---|---|---|
| `lorand`, `hirand` | float 0..1 | 0, 1 | A region is eligible when the note-on event's random number `r` satisfies `lorand <= r < hirand` | 8 |
| `seq_length` | integer 1..100 | 1 | Length of the region's sequence | 7 |
| `seq_position` | integer 1..100 | 1 | The region sounds when its counter equals this | 7 |

Rules, our rule: one random number is drawn per note-on event, shared by all its regions, from a pseudo-random generator seeded with a fixed constant when the backend is created and again on reset, so renders are deterministic. A `hirand` of 1 also admits the largest draw. Each region with `seq_length` above 1 keeps a counter starting at 1, per backend instance (not per channel). Each time all of its other conditions pass, it compares the counter with `seq_position`, sounds on a match, and then advances the counter, wrapping from `seq_length` to 1.

### 5.8 Trigger

| Opcode | Values | Default | Meaning | Survey |
|---|---|---|---|---|
| `trigger` | `attack`, `first`, `legato`, `release`, `release_key` | `attack` | `attack` sounds on note-on. `first` sounds on note-on only when no other note is held on the channel. `legato` only when one is. `release` and `release_key` regions are dropped and listed, never played at note-on | 7 |

Dropping release regions matters: a loader that ignores `trigger` plays string-noise and release samples at the start of every note.

### 5.9 Keyswitches and controller state

OpenTab's stream carries no keyswitch notes, so keyswitch state is fixed.

| Opcode | Type, range | Default | Meaning | Survey |
|---|---|---|---|---|
| `sw_last` | note | none | The region sounds only while this is the active keyswitch | 4 |
| `sw_default` | note | none | The active keyswitch, from any scope (the last one written wins). Our rule: when no scope of the instrument sets it, the active keyswitch is the lowest `sw_last` of the instrument | 3 |
| `sw_lokey`, `sw_hikey` | note | none | The keyswitch range. Parsed and checked, otherwise unused; a note-on inside it is not special | 4 |
| `set_ccN` | integer 0..127, `control` only (elsewhere listed) | none | Initial value of controller N | 9 |
| `loccN`, `hiccN` | integer 0..127 | 0, 127 | The region is eligible only while controller N is inside the inclusive range | 8 |

Controller state is kept per channel. It starts at the `set_ccN` value, else 0, and follows the stream, with two exceptions: controllers 6, 38 and 98 to 101 are consumed as RPN or NRPN data and never change the state, because the stream's RPN 0 bend-range setting would otherwise write 0 into controllers 100 and 101, which surveyed libraries use as ordinary mix switches. Channel mode controllers 120 to 127 are not stored either.

### 5.10 Not in the subset but accepted silently

These carry no sound: `label_ccN`, `*_label` (`region_label`, `group_label`, `master_label`, `global_label`, `sw_label`), `curve_index`, and `<curve>` bodies. They are not listed as ignored.

## 6. Paths

### 6.1 Resolution

The effective sample path is `default_path` followed by the `sample` value, joined as plain text. Then, in order:

1. Every backslash becomes a forward slash. Five surveyed libraries use backslashes for every sample (14,928 regions in one of them); a backslash is never a legal part of a name here.
2. The result must be relative. A leading separator, a drive prefix such as `C:`, a UNC prefix, a NUL byte or an empty result rejects the region.
3. The path is joined to the directory of the root `.sfz` file (not of an included file) and normalised lexically: `.` is dropped, `..` removes the previous component.
4. Spaces and characters such as `#` are ordinary (`A#5_s6_01.flac`, `Kick 1.aif`).

Parent references are normal in real banks: six libraries keep the `.sfz` in a `Programs` folder and the samples in a sibling `Samples` folder, so over 130,000 sample references (after includes) are named `../Samples/...`. A rule that forbids `..` would reject them.

### 6.2 Confinement

The resolved path must stay inside the bank root. After step 3, the path is canonicalised and must start with the canonical bank root; the lexical form is not checked on its own. A region whose path escapes is dropped and listed as "path outside bank"; it is never an error that aborts the bank. The same check applies to every file the loader opens, includes included, and only regular files are opened (never a named pipe or a device): the file is checked without following links, opened, and the opened handle must be the same regular file (same device and inode on Unix).

Symbolic links, our rule: a bank is untrusted user data, so a symbolic link anywhere in the bank tree (to a folder, a file, the program map, or nowhere) refuses the whole bank, and so does a folder or entry the walk cannot read. The folder walk sees every entry before any `.sfz` is read, so no link is ever followed. No surveyed library contains one.

### 6.3 Case

Libraries are written on systems where case does not matter, and OpenTab runs on case-sensitive file systems. When the exact path does not exist, the loader looks in the directory for entries that match ignoring case (both names lowered with Unicode lower-casing), component by component. Exactly one match is used; zero or several is "missing". One surveyed library has a case mismatch (`TIP` against `Tip`). Each directory is listed once, keyed by its canonical path, only after it is confined to the bank, and the entries listed for the whole bank are capped (section 11).

### 6.4 Include paths

An include path is normally quoted; an unquoted path runs to the next white space. It follows steps 1 and 2 above. It is resolved first against the directory of the root `.sfz` file, then, if no such file exists, against the directory of the file that contains the include. The surveyed libraries (eight use includes, 2,875 in total) resolve every nested include against the root file's directory, so the first rule is the one that matters. The result is confined to the bank root exactly as in 6.2.

### 6.5 Audio files

WAV, FLAC and Ogg Vorbis are decoded (M3-D2). The survey found 51,754 `.wav`, 9,388 `.flac` and 88 `.aif` references, no Ogg. WAV files are 16 and 24 bit at 44.1 and 48 kHz, mono and stereo, and some are tagged WAVE_FORMAT_EXTENSIBLE (tag 65534).

- At load, a sample whose extension is not `.wav`, `.flac`, `.ogg` or `.oga` (any case) is dropped and listed as "unsupported audio format"; `.aif` is in this group. At decode time the format is told by the file's first bytes, not by its name.
- WAV: integer samples of 8 to 32 bits and 32-bit float, plain or WAVE_FORMAT_EXTENSIBLE. FLAC: any depth the format allows. Ogg Vorbis: the first logical stream. Values become 32-bit floats in -1..1.
- Samples are shared between regions, and between every track that uses the bank, by canonical path: each file is decoded once per bank.
- Decoding never trusts a declared size: memory grows only with frames actually decoded, under the caps of section 11, so a header that announces hours of audio costs nothing. A truncated or damaged file is invalid. Ogg Vorbis headers are checked before the decoder reads them: their declared comment lengths must fit in the packet, the codebook entries of the file are capped, and an estimate of the memory the decoder will build for its codebooks (lengths, Huffman tree nodes, lookup values and vectors, all from the declared counts) must fit a fixed budget. A WAV whose data chunk claims more bytes than the file holds (some streaming writers leave it at its maximum) is invalid.
- A sample that breaks a cap or does not decode makes its regions silent, and the failure is reported once per bank with its reason. Loading never decodes audio, so such a file does not stop the bank from loading.

## 7. Note names

A note value is an integer 0..127 or a note name: a letter `a` to `g` (either case), at most one accidental (`#` for sharp, `b` for flat), then an octave number, optionally negative. Value: 12 times (octave plus 1), plus the semitone of the letter (c 0, d 2, e 4, f 5, g 7, a 9, b 11), plus 1 for a sharp or minus 1 for a flat. So `c-1` is 0, `c4` is 60 (middle C), `a4` is 69 and `g9` is 127. Names outside 0..127 and a leading `+` are invalid values.

A name written `b#3` or `cb4` is computed by the same arithmetic and then range checked. Names are valid wherever a note is accepted (`key`, `lokey`, `hikey`, `pitch_keycenter`, `sw_*`). One surveyed library uses names (5 files, `c3`, `c#3`); all others use numbers.

The octave convention is C4 = 60. Some SFZ files assume C3 = 60 and compensate with `octave_offset` or `note_offset` in `<control>`. Those two opcodes are not in the subset (no surveyed library uses them) and are ignored and listed. A bank that needs them would sound an octave off, and the listing makes that visible.

## 8. Ignored and listed

Anything not in section 5 is parsed, discarded and recorded, so a user can see what a bank asked for and OpenTab did not do.

- The loader returns, per instrument, a list of entries: opcode, header or directive name (with every run of digits folded to `N`, so `lfo01_freq` becomes `lfoN_freq` and `amplitude_oncc105` becomes `amplitude_onccN`), the scope that set it (file level for headers and directives), the number of occurrences, and a reason. Reasons: `unsupported`, `invalid value`, `clamped`, `unresolved variable`, `path outside bank`, `missing file`, `unsupported audio format`, `release trigger`, `generator`, `disabled region`, `missing sample`, `missing include`, `unknown directive` and `outside header`.
- An ignored opcode behaves as if it were absent. The only things that remove a whole region are the reasons `release trigger`, `generator`, `disabled region` (`end=-1`), `unresolved variable` (in `sample`), `missing sample`, `path outside bank`, `missing file` and `unsupported audio format`; each is counted in the list.
- Controller modulation (`*_onccN`, `*_ccN`, `*_curveccN`, `*_smoothccN`), LFOs (`lfoN_*`, `amplfo_*`, `pitchlfo_*`, `fillfo_*`), envelopes other than amplitude (`egN_*`, `pitcheg_*`, `fileg_*`), filters and equalisers (`cutoff`, `resonance`, `fil_*`, `eqN_*`), `width`, `delay`, `rt_decay`, `offset_random`, `amp_random`, `polyphony`, `note_polyphony`, `xfin_*`, `xfout_*`, `bend_*`, `master_volume`, `group_volume`, `loprog`, `hiprog`, `<effect>`, `<curve>` and `<midi>` are all in this category.
- The list is available per instrument to the bank manager and to the JSON reports built on it, next to the instrument it applies to.

## 9. What the backend does with a bank

- `SfzBank::load` walks the bank root (refusing any symbolic link), finds every `.sfz` file, preprocesses each one to learn which files others include, and parses every file that no other file includes, without decoding audio. Each such root file is an instrument named after its path relative to the root, without extension. A root that is not UTF-8 is reported and skipped.
- Audio is decoded on a region's first note-on. For live output, an instrument can be decoded ahead of time (`preload`), so the audio thread never has to open a file or decode.
- Voices are limited to 128 per backend instance and 32 per note-on event. When the limit is reached, the oldest voice that is releasing, else the oldest voice, is stolen.
- Voices are keyed by MIDI channel: a note-off, pitch bend, all-notes-off or all-sound-off acts on the voices of its channel only.
- Program change, bank select and the `loprog` and `hiprog` opcodes do not reach the SFZ text: they choose the instrument through the program map (section 10).

## 10. Program map

SFZ files carry no program numbers, so a bank maps General MIDI programs to its instruments (M3 preflight F12).

- When the bank root holds `opentab-bank.json`, written when the bank is imported, it is the map. It is a JSON object with `version` (1), `programs` (a list of objects with `program` 0..127 and `file`, a root `.sfz` path relative to the bank root with `/` separators), and optional `drums` and `default` files. Unknown fields, another version, a program above 127 or a file that is not an instrument of the bank make the map invalid, and the bank is refused with that reason.
- Without the file, our rule: the first instrument in path order whose path does not contain `drum`, `kit` or `perc` (any case) serves every program, and drums use the first instrument whose path contains one of them. A bank with no such file plays no drums, so the bank manager can fall back to the built-in kit; a bank with only drum-named files serves programs from its first file.
- A program the map does not list uses `default`, else the first-instrument rule. A channel plays drums when it is channel 10 or its bank select MSB is 120, as everywhere in OpenTab.
- The bank lists each instrument once: the drum instrument under bank 120, the others under bank 0, each with the lowest program the map gives it, else program 0.

## 11. Limits

Every limit is a named constant and refuses the bank with a message that names it. Sizes are checked before the memory they guard is allocated; counts are checked as each item is added. Loading work is bounded by these limits rather than by the bank's shape: comments are removed in one pass; a variable is looked up only among the lengths of defined names, each at most 64 bytes; each path, after variables, is at most 1 KiB and its text counts against the bank text limit; each distinct path is resolved once (hits and misses are both remembered), and the number of distinct paths is capped; each folder is listed and lower-cased once into an index, so a case-insensitive miss costs one lookup in that index. The remembered results are charged to the memory kept per bank. The total work is roughly proportional to the text read, plus the folder entries listed, plus each path occurrence times the length of its key and canonical result (a hit still builds and hashes its key; an include hit also checks, opens and re-checks the file, a few system calls on a path of at most about 4 KiB), with path occurrences bounded by the region and include limits. Real libraries sit far below each one: the survey's largest file is 210 KB, its largest instrument 8,902 regions and 0.95 MB of expanded text, its deepest include chain 3 levels, and its longest samples a few tens of seconds.

| Limit | Value |
|---|---|
| Instrument text file (`.sfz` or included) | 4 MiB |
| Expanded text per instrument | 16 MiB |
| Text read and expanded per bank, every file counted | 512 MiB |
| Regions per instrument | 100,000 |
| Include depth, includes per instrument | 8, 4,096 |
| Include occurrences per bank, both passes counted | 16,384 (the survey's 8 libraries hold 2,875 include lines in all) |
| Definitions per instrument, their text, one name | 16,384, 1 MiB, 64 bytes (a longer name is ignored and listed) |
| One line of text | 64 KiB |
| One sample or include path, after variables and `default_path` | 1 KiB |
| Distinct path lookups per bank | 65,536 |
| Text read per instrument, includes counted each time | 32 MiB |
| Folder entries listed for case-insensitive lookups, per bank | 1,048,576 |
| Entries in the bank folder, folder depth | 200,000, 16 |
| `.sfz` files in the bank | 8,192 |
| Memory kept for regions and lists, per bank | 256 MiB |
| `opentab-bank.json` | 1 MiB |
| Sample file | 256 MiB |
| Decoded sample length | 16,777,216 frames (about 6 minutes at 44.1 kHz) |
| Sample channels, sample rate | 1 or 2, 8,000 to 192,000 Hz |
| Decoded audio per bank | 1 GiB |
| Ogg Vorbis codebook entries per file | 262,144 |
| Ogg Vorbis codebook memory estimate per file | 32 MiB |
| Ogg Vorbis comment header estimate per file (the vendor text, 48 bytes per declared comment, plus twice its text) | 1 MiB |
| Logical streams in an Ogg file (every page header is checked before the Ogg reader runs) | 1 |

The sample limits are checked when a sample is decoded; a breach silences its regions and is reported, as in section 6.5.
