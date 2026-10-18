const MUSICXML_OPEN: &[u8] = b"<encoding-date>";
const MUSICXML_CLOSE: &[u8] = b"</encoding-date>";
const MUSICXML_EPOCH: &[u8] = b"1970-01-01";
const PDF_DATE_KEYS: [&[u8]; 2] = [b"/CreationDate", b"/ModDate"];
const PDF_EPOCH_TEXT: &[u8] = b"D:19700101000000Z";

pub fn normalize_musicxml(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while let Some(found) = find(&input[pos..], MUSICXML_OPEN) {
        let content_start = pos + found + MUSICXML_OPEN.len();
        let Some(close) = find(&input[content_start..], MUSICXML_CLOSE) else {
            break;
        };
        out.extend_from_slice(&input[pos..content_start]);
        out.extend_from_slice(MUSICXML_EPOCH);
        pos = content_start + close;
    }
    out.extend_from_slice(&input[pos..]);
    out
}

pub fn normalize_pdf(input: &[u8]) -> Vec<u8> {
    let mut current = input.to_vec();
    for key in PDF_DATE_KEYS {
        current = replace_pdf_date(&current, key);
    }
    current
}

pub fn normalize_output(path: &str, input: &[u8]) -> Vec<u8> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".musicxml") || lower.ends_with(".xml") {
        normalize_musicxml(input)
    } else if lower.ends_with(".pdf") {
        normalize_pdf(input)
    } else {
        input.to_vec()
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn is_pdf_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0')
}

fn is_pdf_delimiter(byte: u8) -> bool {
    is_pdf_whitespace(byte)
        || matches!(
            byte,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
}

fn literal_string_end(input: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < input.len() {
        match input[index] {
            b'\\' => index += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn hex_string_end(input: &[u8], open: usize) -> Option<usize> {
    let rest = input.get(open + 1..)?;
    if rest.first() == Some(&b'<') {
        return None;
    }
    find(rest, b">").map(|close| open + 1 + close + 1)
}

fn pdf_value_end(input: &[u8], start: usize) -> Option<usize> {
    match input.get(start)? {
        b'(' => literal_string_end(input, start),
        b'<' => hex_string_end(input, start),
        _ => None,
    }
}

fn fit(text: &[u8], len: usize) -> Vec<u8> {
    let mut fitted = text[..text.len().min(len)].to_vec();
    fitted.resize(len, b' ');
    fitted
}

fn hex_digits(text: &[u8]) -> Vec<u8> {
    let mut digits = Vec::with_capacity(text.len() * 2);
    for byte in text {
        digits.extend_from_slice(format!("{byte:02X}").as_bytes());
    }
    digits
}

fn push_epoch_value(out: &mut Vec<u8>, value: &[u8]) {
    let inner = value.len().saturating_sub(2);
    out.push(value[0]);
    if value[0] == b'(' {
        out.extend_from_slice(&fit(PDF_EPOCH_TEXT, inner));
    } else {
        out.extend_from_slice(&fit(&hex_digits(PDF_EPOCH_TEXT), inner));
    }
    out.push(value[value.len() - 1]);
}

fn replace_pdf_date(input: &[u8], key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while let Some(found) = find(&input[pos..], key) {
        let key_end = pos + found + key.len();
        let terminated = input
            .get(key_end)
            .is_none_or(|byte| is_pdf_delimiter(*byte));
        let mut value_start = key_end;
        while input
            .get(value_start)
            .is_some_and(|byte| is_pdf_whitespace(*byte))
        {
            value_start += 1;
        }
        let value_end = if terminated {
            pdf_value_end(input, value_start)
        } else {
            None
        };
        match value_end {
            Some(end) => {
                out.extend_from_slice(&input[pos..value_start]);
                push_epoch_value(&mut out, &input[value_start..end]);
                pos = end;
            }
            None => {
                out.extend_from_slice(&input[pos..key_end]);
                pos = key_end;
            }
        }
    }
    out.extend_from_slice(&input[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::{normalize_musicxml, normalize_output, normalize_pdf};

    fn pdf_with(creation: &str, modified: &str) -> Vec<u8> {
        format!(
            "%PDF-1.4\n1 0 obj\n<< /Producer (Guitar Pro 8) /CreationDate {creation} /ModDate{modified} >>\nendobj\nxref\n0 2\n0000000000 65535 f \n0000000009 00000 n \ntrailer\n<< /Info 1 0 R >>\nstartxref\n120\n%%EOF\n"
        )
        .into_bytes()
    }

    fn date_strings() -> Vec<String> {
        let mut dates = Vec::new();
        for year in [1999u32, 2026, 2031] {
            for month in [1u32, 7, 12] {
                for second in [0u32, 17, 59] {
                    dates.push(format!("(D:{year}{month:02}0{}2133{second:02})", month % 9));
                    dates.push(format!("(D:{year}{month:02}17213301-03'00')"));
                    dates.push(format!("(D:{year}{month:02}17{second:02}Z)"));
                    dates.push(format!("<FEFF00440{}3A{year:04}>", month % 10));
                }
            }
        }
        dates.push(String::from("(D:2026)"));
        dates.push(String::from("(D:20261017\\)x)"));
        dates
    }

    fn changed_positions(before: &[u8], after: &[u8]) -> Vec<usize> {
        (0..before.len())
            .filter(|at| before[*at] != after[*at])
            .collect()
    }

    #[test]
    fn pdfs_with_equal_length_dates_normalise_identically() {
        let dates = date_strings();
        for creation in &dates {
            for modified in &dates {
                let original = pdf_with(creation, modified);
                let normalised = normalize_pdf(&original);
                assert!(!normalised.windows(4).any(|w| w == b"2026"));
                assert_eq!(
                    normalize_pdf(&normalised),
                    normalised,
                    "{creation} {modified}"
                );
                for other in dates.iter().filter(|d| {
                    d.len() == creation.len() && d.as_bytes()[0] == creation.as_bytes()[0]
                }) {
                    let twin = normalize_pdf(&pdf_with(other, modified));
                    assert_eq!(twin, normalised, "{creation} vs {other}");
                }
            }
        }
    }

    #[test]
    fn pdf_normalisation_preserves_length_and_offsets_for_any_date_length() {
        let mut dates = date_strings();
        for len in 0..40usize {
            dates.push(format!("({})", "9".repeat(len)));
            dates.push(format!("<{}>", "A".repeat(len)));
        }
        for creation in &dates {
            for modified in dates.iter().step_by(7) {
                let original = pdf_with(creation, modified);
                let normalised = normalize_pdf(&original);
                assert_eq!(normalised.len(), original.len(), "{creation} {modified}");
                let find = |bytes: &[u8], needle: &[u8]| {
                    bytes.windows(needle.len()).position(|w| w == needle)
                };
                for marker in [&b"xref"[..], b"trailer", b"startxref", b"endobj", b"%%EOF"] {
                    assert_eq!(find(&normalised, marker), find(&original, marker));
                }
                let first = original
                    .windows(13)
                    .position(|w| w == b"/CreationDate")
                    .expect("key");
                let outside = changed_positions(&original[..first], &normalised[..first]);
                assert!(outside.is_empty(), "{creation} {modified}");
            }
        }
    }

    #[test]
    fn short_pdf_dates_become_a_prefix_of_the_epoch_not_a_longer_value() {
        let original = pdf_with("(D:2026)", "(x)");
        let normalised = normalize_pdf(&original);
        assert_eq!(normalised.len(), original.len());
        let text = String::from_utf8(normalised).expect("ascii");
        assert!(text.contains("/CreationDate (D:1970)"));
        assert!(text.contains("/ModDate(D)"));
    }

    #[test]
    fn pdf_dates_become_epoch() {
        let normalised = normalize_pdf(&pdf_with(
            "(D:20261017213301-03'00')",
            "(D:20261018000000Z)",
        ));
        let text = String::from_utf8(normalised).expect("ascii");
        assert!(text.contains("/CreationDate (D:19700101000000Z "));
        assert!(text.contains("/ModDate(D:19700101000000Z)"));
        assert!(!text.contains("20261017"));
        assert!(!text.contains("20261018"));
    }

    #[test]
    fn pdf_leaves_other_names_and_bare_keys_alone() {
        let input = b"<< /ModDateX (D:2026) /CreationDate 12 /Title (D:2026) >> /ModDate".to_vec();
        assert_eq!(normalize_pdf(&input), input);
    }

    #[test]
    fn pdf_without_dates_is_unchanged() {
        let input = b"%PDF-1.4\n<< /Producer (x) >>\n%%EOF\n".to_vec();
        assert_eq!(normalize_pdf(&input), input);
    }

    #[test]
    fn musicxml_dates_become_epoch_and_are_idempotent() {
        let make = |date: &str| {
            format!(
                "<?xml version=\"1.0\"?>\n<identification><encoding><software>Guitar Pro 8.1.5</software><encoding-date>{date}</encoding-date></encoding></identification>\n<encoding-date>{date}</encoding-date>"
            )
            .into_bytes()
        };
        let reference = normalize_musicxml(&make("1970-01-01"));
        for date in ["2026-10-17", "2027-01-01", "", "garbage date"] {
            let normalised = normalize_musicxml(&make(date));
            assert_eq!(normalised, reference, "{date}");
            assert_eq!(normalize_musicxml(&normalised), normalised);
        }
        let text = String::from_utf8(reference).expect("utf8");
        assert_eq!(
            text.matches("<encoding-date>1970-01-01</encoding-date>")
                .count(),
            2
        );
    }

    #[test]
    fn musicxml_unterminated_tag_is_left_alone() {
        let input = b"<a><encoding-date>2026-10-17".to_vec();
        assert_eq!(normalize_musicxml(&input), input);
    }

    #[test]
    fn output_dispatch_by_extension() {
        let xml = b"<encoding-date>2026-10-17</encoding-date>";
        assert_eq!(
            normalize_output("export.musicxml", xml),
            b"<encoding-date>1970-01-01</encoding-date>".to_vec()
        );
        assert_eq!(normalize_output("export.mid", xml), xml.to_vec());
        assert_eq!(normalize_output("resave.gp", xml), xml.to_vec());
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\x07tIME\x07\xea\x0a\x11\x15\x21\x00\0\0\0\0";
        assert_eq!(normalize_output("png/a#01.png", png), png.to_vec());
    }
}
