const MUSICXML_OPEN: &[u8] = b"<encoding-date>";
const MUSICXML_CLOSE: &[u8] = b"</encoding-date>";
const MUSICXML_EPOCH: &[u8] = b"1970-01-01";
const PDF_DATE_KEYS: [&[u8]; 2] = [b"/CreationDate", b"/ModDate"];
const PDF_EPOCH: &[u8] = b"(D:19700101000000Z)";
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PNG_VOLATILE_CHUNKS: [&[u8]; 1] = [b"tIME"];

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

pub fn normalize_png(input: &[u8]) -> Vec<u8> {
    match strip_png_chunks(input) {
        Some(stripped) => stripped,
        None => input.to_vec(),
    }
}

pub fn normalize_output(path: &str, input: &[u8]) -> Vec<u8> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".musicxml") || lower.ends_with(".xml") {
        normalize_musicxml(input)
    } else if lower.ends_with(".pdf") {
        normalize_pdf(input)
    } else if lower.ends_with(".png") {
        normalize_png(input)
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
                out.extend_from_slice(PDF_EPOCH);
                let old_len = end - value_start;
                if old_len > PDF_EPOCH.len() {
                    out.resize(out.len() + old_len - PDF_EPOCH.len(), b' ');
                }
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

fn strip_png_chunks(input: &[u8]) -> Option<Vec<u8>> {
    let body = input.strip_prefix(PNG_SIGNATURE)?;
    let mut out = Vec::with_capacity(input.len());
    out.extend_from_slice(PNG_SIGNATURE);
    let mut pos = 0;
    while pos < body.len() {
        let length_bytes: [u8; 4] = body.get(pos..pos + 4)?.try_into().ok()?;
        let length = usize::try_from(u32::from_be_bytes(length_bytes)).ok()?;
        let chunk_end = pos.checked_add(12)?.checked_add(length)?;
        let chunk = body.get(pos..chunk_end)?;
        let kind = &chunk[4..8];
        if !PNG_VOLATILE_CHUNKS.contains(&kind) {
            out.extend_from_slice(chunk);
        }
        pos = chunk_end;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{PDF_EPOCH, normalize_musicxml, normalize_output, normalize_pdf, normalize_png};

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

    #[test]
    fn pdfs_differing_only_in_dates_normalise_identically() {
        let dates = date_strings();
        let reference = normalize_pdf(&pdf_with(&dates[0], &dates[1]));
        for creation in &dates {
            for modified in &dates {
                let original = pdf_with(creation, modified);
                let normalised = normalize_pdf(&original);
                assert!(!normalised.windows(4).any(|w| w == b"2026"));
                let stripped_reference: Vec<u8> =
                    reference.iter().copied().filter(|b| *b != b' ').collect();
                let stripped: Vec<u8> = normalised.iter().copied().filter(|b| *b != b' ').collect();
                assert_eq!(stripped, stripped_reference, "{creation} {modified}");
                assert_eq!(normalize_pdf(&normalised), normalised);
            }
        }
    }

    #[test]
    fn pdf_dates_keep_byte_offsets_when_not_shorter_than_epoch() {
        let dates = date_strings();
        for creation in dates.iter().filter(|d| d.len() >= PDF_EPOCH.len()) {
            for modified in dates.iter().filter(|d| d.len() >= PDF_EPOCH.len()) {
                let original = pdf_with(creation, modified);
                let normalised = normalize_pdf(&original);
                assert_eq!(normalised.len(), original.len());
                let xref_at = |bytes: &[u8]| bytes.windows(4).position(|w| w == b"xref");
                assert_eq!(xref_at(&normalised), xref_at(&original));
            }
        }
    }

    #[test]
    fn pdf_dates_become_epoch() {
        let normalised = normalize_pdf(&pdf_with(
            "(D:20261017213301-03'00')",
            "(D:20261018000000Z)",
        ));
        let text = String::from_utf8(normalised).expect("ascii");
        assert!(text.contains("/CreationDate (D:19700101000000Z)"));
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

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&u32::try_from(data.len()).expect("small").to_be_bytes());
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&[0, 0, 0, 0]);
        chunk
    }

    fn png_with(time: Option<&[u8]>) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]));
        if let Some(time) = time {
            png.extend(png_chunk(b"tIME", time));
        }
        png.extend(png_chunk(b"IDAT", b"pixels"));
        png.extend(png_chunk(b"IEND", b""));
        png
    }

    #[test]
    fn png_time_chunks_are_removed() {
        let reference = png_with(None);
        for second in 0u8..60 {
            let stamped = png_with(Some(&[0x07, 0xea, 10, 17, 21, 33, second]));
            assert_eq!(normalize_png(&stamped), reference);
        }
        assert_eq!(normalize_png(&reference), reference);
    }

    #[test]
    fn malformed_png_is_left_alone() {
        let mut truncated = png_with(Some(&[1, 2, 3, 4, 5, 6, 7]));
        truncated.truncate(truncated.len() - 3);
        assert_eq!(normalize_png(&truncated), truncated);
        assert_eq!(normalize_png(b"not a png"), b"not a png".to_vec());
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
    }
}
