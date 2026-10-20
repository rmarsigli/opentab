use std::io::{Cursor, Write};

use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipItem {
    File { name: String, data: Vec<u8> },
    Stored { name: String, data: Vec<u8> },
    Folder { name: String },
    Link { name: String, target: String },
}

pub fn zip_archive(items: &[ZipItem]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for item in items {
        let outcome = match item {
            ZipItem::File { name, data } => writer
                .start_file(name.as_str(), deflated)
                .and_then(|()| writer.write_all(data).map_err(Into::into)),
            ZipItem::Stored { name, data } => writer
                .start_file(name.as_str(), stored)
                .and_then(|()| writer.write_all(data).map_err(Into::into)),
            ZipItem::Folder { name } => writer.add_directory(name.as_str(), stored),
            ZipItem::Link { name, target } => {
                writer.add_symlink(name.as_str(), target.as_str(), stored)
            }
        };
        if let Err(error) = outcome {
            panic!("cannot write the test zip: {error}");
        }
    }
    match writer.finish() {
        Ok(cursor) => cursor.into_inner(),
        Err(error) => panic!("cannot finish the test zip: {error}"),
    }
}
