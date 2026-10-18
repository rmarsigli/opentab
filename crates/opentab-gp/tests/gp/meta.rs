use crate::support::{Doc, document_payloads, read_gpif};

const FIELDS: [&str; 10] = [
    "SubTitle",
    "Artist",
    "Album",
    "Words",
    "Music",
    "Copyright",
    "Tabber",
    "Instructions",
    "Notices",
    "WordsAndMusic",
];

#[test]
fn every_metadata_field_is_mapped() {
    let score: String = FIELDS
        .iter()
        .map(|name| format!("<{name}><![CDATA[{name} text]]></{name}>"))
        .collect();
    let doc = Doc {
        score,
        ..Doc::default()
    };
    let score = doc.read();
    let meta = &score.meta;
    assert_eq!(meta.title, "Song");
    assert_eq!(meta.subtitle, "SubTitle text");
    assert_eq!(meta.artist, "Artist text");
    assert_eq!(meta.album, "Album text");
    assert_eq!(meta.words, "Words text");
    assert_eq!(meta.music, "Music text");
    assert_eq!(meta.copyright, "Copyright text");
    assert_eq!(meta.tab_author, "Tabber text");
    assert_eq!(meta.instructions, "Instructions text");
    assert_eq!(meta.notices, "Notices text");
    assert!(document_payloads(&score).contains(
        &"<Keep parent=\"Score\" index=\"10\"><WordsAndMusic><![CDATA[WordsAndMusic text]]></WordsAndMusic></Keep>"
            .to_string()
    ));
}

#[test]
fn text_joins_cdata_sections_and_ignores_layout_whitespace() {
    let doc = Doc {
        score: "<Artist>\n<![CDATA[AC/]]><![CDATA[DC]]>\n</Artist><Album>R&amp;B &lt;live&gt; &#233;</Album><Words><![CDATA[ a ]]]]><![CDATA[> b ]]></Words>".to_string(),
        ..Doc::default()
    };
    let score = doc.read();
    assert_eq!(score.meta.artist, "AC/DC");
    assert_eq!(score.meta.album, "R&B <live> \u{e9}");
    assert_eq!(score.meta.words, " a ]]> b ");
}

#[test]
fn non_ascii_text_survives() {
    let gpif = Doc::default()
        .render()
        .replace("<![CDATA[Song]]>", "<![CDATA[Canção 🎸 <&>]]>");
    assert_eq!(read_gpif(&gpif).meta.title, "Canção 🎸 <&>");
}

#[test]
fn root_level_unknowns_are_document_payloads_with_their_position() {
    let doc = Doc {
        root: "<GPVersion>7.6.0</GPVersion><BackingTrack><Name><![CDATA[x]]></Name></BackingTrack>"
            .to_string(),
        ..Doc::default()
    };
    let payloads = document_payloads(&doc.read());
    assert_eq!(
        payloads[0],
        "<Keep parent=\"\" index=\"0\"><GPRevision>13008</GPRevision></Keep>"
    );
    assert_eq!(
        payloads[1],
        "<Keep parent=\"\" index=\"1\"><GPVersion>7.6.0</GPVersion></Keep>"
    );
    assert_eq!(
        payloads[2],
        "<Keep parent=\"\" index=\"2\"><BackingTrack><Name><![CDATA[x]]></Name></BackingTrack></Keep>"
    );
    let current = Doc {
        root: "<GPVersion>8.1.5</GPVersion>".to_string(),
        ..Doc::default()
    };
    let payloads = document_payloads(&current.read());
    assert_eq!(
        payloads,
        vec!["<Keep parent=\"\" index=\"0\"><GPRevision>13008</GPRevision></Keep>".to_string()]
    );
}

#[test]
fn unknown_attributes_of_mapped_elements_are_kept() {
    let gpif = Doc::default().render().replace(
        "<Track id=\"0\">",
        "<Track id=\"0\" colour=\"a&quot;b&#10;c\">",
    );
    let score = read_gpif(&gpif);
    let payloads = crate::support::payloads(&score, score.tracks[0].id.0);
    assert_eq!(
        payloads[0],
        "<KeepAttribute parent=\"\" index=\"1\" name=\"colour\" value=\"a&quot;b&#10;c\"/>"
    );
}
