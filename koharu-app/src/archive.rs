use std::io::{Cursor, Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use koharu_core::ArchiveFormat;
use koharu_core::commands::FileEntry;

const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const SEVEN_Z_MAGIC: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];

const IMAGE_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".webp"];

/// Expand any archives in `files` into individual image [`FileEntry`] values.
/// Non-archive files pass through unchanged.
pub fn expand_archives(files: Vec<FileEntry>) -> Result<Vec<FileEntry>> {
    let mut out = Vec::new();

    for file in files {
        if is_zip(&file.data) {
            let extracted = extract_images_from_zip(&file)
                .with_context(|| format!("failed to read archive `{}`", file.name))?;
            out.extend(extracted);
        } else if is_7z(&file.data) {
            let extracted = extract_images_from_7z(&file)
                .with_context(|| format!("failed to read archive `{}`", file.name))?;
            out.extend(extracted);
        } else {
            out.push(file);
        }
    }

    Ok(out)
}

fn is_zip(data: &[u8]) -> bool {
    data.len() >= ZIP_MAGIC.len() && data[..ZIP_MAGIC.len()] == ZIP_MAGIC
}

fn is_7z(data: &[u8]) -> bool {
    data.len() >= SEVEN_Z_MAGIC.len() && data[..SEVEN_Z_MAGIC.len()] == SEVEN_Z_MAGIC
}

fn extract_images_from_zip(file: &FileEntry) -> Result<Vec<FileEntry>> {
    let cursor = Cursor::new(&file.data);
    let mut archive = zip::ZipArchive::new(cursor).context("not a valid ZIP file")?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read entry at index {index}"))?;

        if entry.is_dir() {
            continue;
        }

        let entry_path = match entry.enclosed_name() {
            Some(p) => p.to_string_lossy().to_string(),
            None => continue,
        };

        if !should_extract_image(&entry_path) {
            continue;
        }

        let mut data = Vec::with_capacity(entry.size() as usize);
        if let Err(e) = entry.read_to_end(&mut data) {
            tracing::warn!(
                archive = %file.name,
                entry = %entry_path,
                error = %e,
                "skipping unreadable archive entry",
            );
            continue;
        }

        entries.push((entry_path, data));
    }

    collected_to_file_entries(&file.name, entries)
}

fn extract_images_from_7z(file: &FileEntry) -> Result<Vec<FileEntry>> {
    let cursor = Cursor::new(&file.data);
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();

    sevenz_rust::decompress_with_extract_fn(cursor, "", |entry, reader, _dest| {
        if entry.is_directory() {
            return Ok(true);
        }

        let entry_path = entry.name().to_string();

        if !should_extract_image(&entry_path) {
            return Ok(true);
        }

        let mut data = Vec::with_capacity(entry.size() as usize);
        if let Err(e) = reader.read_to_end(&mut data) {
            tracing::warn!(
                archive = %file.name,
                entry = %entry_path,
                error = %e,
                "skipping unreadable archive entry",
            );
            return Ok(true);
        }

        entries.push((entry_path, data));
        Ok(true)
    })
    .context("not a valid 7z file")?;

    collected_to_file_entries(&file.name, entries)
}

/// Turn collected (path, data) pairs into sorted, flat-named [`FileEntry`] values.
fn collected_to_file_entries(
    archive_name: &str,
    mut entries: Vec<(String, Vec<u8>)>,
) -> Result<Vec<FileEntry>> {
    if entries.is_empty() {
        bail!(
            "archive `{}` contains no supported image files",
            archive_name
        );
    }

    entries.sort_by(|(a, _), (b, _)| natord::compare(a, b));

    let needs_prefix = has_duplicate_basenames(&entries);

    let result = entries
        .into_iter()
        .map(|(archive_path, data)| {
            let name = flat_name(&archive_path, needs_prefix);
            FileEntry { name, data }
        })
        .collect();

    Ok(result)
}

fn should_extract_image(entry_path: &str) -> bool {
    if entry_path.starts_with("__MACOSX/") || entry_path.contains("/__MACOSX/") {
        return false;
    }

    let file_name = match Path::new(entry_path).file_name() {
        Some(n) => n.to_string_lossy(),
        None => return false,
    };

    if file_name.starts_with('.') || file_name.eq_ignore_ascii_case("Thumbs.db") {
        return false;
    }

    let lower = file_name.to_ascii_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

fn has_duplicate_basenames(entries: &[(String, Vec<u8>)]) -> bool {
    let mut seen = std::collections::HashSet::new();
    for (path, _) in entries {
        let base = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if !seen.insert(base) {
            return true;
        }
    }
    false
}

/// `Chapter01/001.jpg` -> `Chapter01_001.jpg` (prefix) or `001.jpg` (no prefix).
fn flat_name(archive_path: &str, prefix: bool) -> String {
    let path = Path::new(archive_path);

    if !prefix {
        return path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| archive_path.to_string());
    }

    let components: Vec<&str> = archive_path.split('/').filter(|s| !s.is_empty()).collect();

    if components.len() <= 1 {
        return components
            .first()
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| archive_path.to_string());
    }

    components.join("_")
}

// ── Archive Export ──────────────────────────────────────────────────

pub struct ExportEntry {
    pub filename: String,
    pub data: Vec<u8>,
}

pub fn create_archive(entries: &[ExportEntry], format: ArchiveFormat) -> Result<Vec<u8>> {
    match format {
        ArchiveFormat::Cbz => create_cbz(entries),
        ArchiveFormat::Cb7 => create_cb7(entries),
    }
}

fn create_cbz(entries: &[ExportEntry]) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for entry in entries {
        writer
            .start_file(&entry.filename, options)
            .with_context(|| format!("failed to write entry `{}`", entry.filename))?;
        writer.write_all(&entry.data)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn create_cb7(entries: &[ExportEntry]) -> Result<Vec<u8>> {
    let dir = tempfile::tempdir().context("failed to create temp dir for 7z export")?;
    for entry in entries {
        let path = dir.path().join(&entry.filename);
        std::fs::write(&path, &entry.data)
            .with_context(|| format!("failed to write temp file `{}`", entry.filename))?;
    }
    let out_path = dir.path().join("archive.7z");
    sevenz_rust::compress_to_path(dir.path(), &out_path).context("failed to create 7z archive")?;
    std::fs::read(&out_path).context("failed to read 7z archive")
}

pub fn archive_extension(format: ArchiveFormat) -> &'static str {
    match format {
        ArchiveFormat::Cbz => "cbz",
        ArchiveFormat::Cb7 => "cb7",
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn tiny_png() -> Vec<u8> {
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            &[255u8, 0, 0, 255],
            1,
            1,
            image::ColorType::Rgba8.into(),
        )
        .unwrap();
        buf
    }

    fn tiny_jpeg() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0]));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    }

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = Vec::new();
        let cursor = Cursor::new(buf);
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn make_7z(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        for (name, data) in entries {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, data).unwrap();
        }
        let out_path = dir.path().join("test.7z");
        sevenz_rust::compress_to_path(dir.path(), &out_path).unwrap();
        std::fs::read(&out_path).unwrap()
    }

    #[test]
    fn extracts_images_from_valid_cbz() {
        let png = tiny_png();
        let jpeg = tiny_jpeg();
        let zip_data = make_zip(&[("page1.png", &png), ("page2.jpg", &jpeg)]);

        let files = vec![FileEntry {
            name: "manga.cbz".into(),
            data: zip_data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "page1.png");
        assert_eq!(result[1].name, "page2.jpg");
        assert_eq!(result[0].data, png);
        assert_eq!(result[1].data, jpeg);
    }

    #[test]
    fn skips_non_image_entries() {
        let png = tiny_png();
        let zip_data = make_zip(&[
            ("page1.png", &png),
            ("ComicInfo.xml", b"<ComicInfo/>"),
            ("__MACOSX/._page1.png", b"junk"),
            (".DS_Store", b"junk"),
            ("Thumbs.db", b"junk"),
            ("readme.txt", b"hello"),
        ]);

        let files = vec![FileEntry {
            name: "test.cbz".into(),
            data: zip_data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "page1.png");
    }

    #[test]
    fn flattens_nested_folders_with_prefix() {
        let png = tiny_png();
        let zip_data = make_zip(&[
            ("Chapter01/001.jpg", &png),
            ("Chapter01/002.jpg", &png),
            ("Chapter02/001.jpg", &png),
        ]);

        let files = vec![FileEntry {
            name: "manga.cbz".into(),
            data: zip_data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "Chapter01_001.jpg");
        assert_eq!(result[1].name, "Chapter01_002.jpg");
        assert_eq!(result[2].name, "Chapter02_001.jpg");
    }

    #[test]
    fn no_prefix_when_basenames_unique() {
        let png = tiny_png();
        let zip_data = make_zip(&[("images/page01.png", &png), ("images/page02.png", &png)]);

        let files = vec![FileEntry {
            name: "manga.cbz".into(),
            data: zip_data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "page01.png");
        assert_eq!(result[1].name, "page02.png");
    }

    #[test]
    fn natural_sort_ordering() {
        let png = tiny_png();
        let zip_data = make_zip(&[
            ("page10.png", &png),
            ("page2.png", &png),
            ("page1.png", &png),
        ]);

        let files = vec![FileEntry {
            name: "manga.cbz".into(),
            data: zip_data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result[0].name, "page1.png");
        assert_eq!(result[1].name, "page2.png");
        assert_eq!(result[2].name, "page10.png");
    }

    #[test]
    fn empty_archive_returns_error() {
        let zip_data = make_zip(&[("readme.txt", b"no images here")]);

        let files = vec![FileEntry {
            name: "empty.cbz".into(),
            data: zip_data,
        }];

        let err = expand_archives(files).unwrap_err();
        let chain = format!("{err:#}");
        assert!(
            chain.contains("no supported image files"),
            "unexpected error: {chain}",
        );
    }

    #[test]
    fn invalid_zip_returns_error() {
        let mut bad = ZIP_MAGIC.to_vec();
        bad.extend_from_slice(b"not-a-real-archive");

        let files = vec![FileEntry {
            name: "corrupt.cbz".into(),
            data: bad,
        }];

        let err = expand_archives(files).unwrap_err();
        assert!(
            err.to_string().contains("corrupt.cbz"),
            "error should mention file name: {err}",
        );
    }

    #[test]
    fn regular_images_pass_through() {
        let png = tiny_png();
        let files = vec![FileEntry {
            name: "page.png".into(),
            data: png.clone(),
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "page.png");
        assert_eq!(result[0].data, png);
    }

    #[test]
    fn mixed_archives_and_images() {
        let png = tiny_png();
        let zip_data = make_zip(&[("inside.png", &png)]);

        let files = vec![
            FileEntry {
                name: "standalone.jpg".into(),
                data: png.clone(),
            },
            FileEntry {
                name: "manga.cbz".into(),
                data: zip_data,
            },
        ];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "standalone.jpg");
        assert_eq!(result[1].name, "inside.png");
    }

    #[test]
    fn is_zip_detects_magic_bytes() {
        assert!(is_zip(&ZIP_MAGIC));
        assert!(is_zip(&[0x50, 0x4B, 0x03, 0x04, 0x00]));
        assert!(!is_zip(b"PNG"));
        assert!(!is_zip(&[]));
    }

    #[test]
    fn is_7z_detects_magic_bytes() {
        assert!(is_7z(&SEVEN_Z_MAGIC));
        assert!(is_7z(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00]));
        assert!(!is_7z(b"PK"));
        assert!(!is_7z(&[]));
    }

    #[test]
    fn extracts_images_from_7z() {
        let png = tiny_png();
        let data = make_7z(&[("page1.png", &png), ("page2.png", &png)]);

        let files = vec![FileEntry {
            name: "manga.cb7".into(),
            data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].name.contains("page1"));
        assert!(result[1].name.contains("page2"));
    }

    #[test]
    fn invalid_7z_returns_error() {
        let mut bad = SEVEN_Z_MAGIC.to_vec();
        bad.extend_from_slice(b"not-a-real-archive");

        let files = vec![FileEntry {
            name: "corrupt.cb7".into(),
            data: bad,
        }];

        let err = expand_archives(files).unwrap_err();
        assert!(
            err.to_string().contains("corrupt.cb7"),
            "error should mention file name: {err}",
        );
    }

    #[test]
    fn mixed_cbz_and_cb7_archives() {
        let png = tiny_png();
        let zip_data = make_zip(&[("from_zip.png", &png)]);
        let sz_data = make_7z(&[("from_7z.png", &png)]);

        let files = vec![
            FileEntry {
                name: "vol1.cbz".into(),
                data: zip_data,
            },
            FileEntry {
                name: "vol2.cb7".into(),
                data: sz_data,
            },
        ];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "from_zip.png");
        assert!(result[1].name.contains("from_7z"));
    }

    #[test]
    fn natural_sort_ordering_7z() {
        let png = tiny_png();
        let data = make_7z(&[
            ("page10.png", &png),
            ("page2.png", &png),
            ("page1.png", &png),
        ]);

        let files = vec![FileEntry {
            name: "manga.cb7".into(),
            data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].name.contains("page1"));
        assert!(result[1].name.contains("page2"));
        assert!(result[2].name.contains("page10"));
    }

    #[test]
    fn nested_folders_in_7z() {
        let png = tiny_png();
        let data = make_7z(&[
            ("ch01/001.jpg", &png),
            ("ch01/002.jpg", &png),
            ("ch02/001.jpg", &png),
        ]);

        let files = vec![FileEntry {
            name: "manga.cb7".into(),
            data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].name.contains("ch01") && result[0].name.contains("001"));
        assert!(result[2].name.contains("ch02"));
    }

    #[test]
    fn skips_non_image_entries_in_7z() {
        let png = tiny_png();
        let data = make_7z(&[
            ("page1.png", &png),
            ("ComicInfo.xml", b"<ComicInfo/>"),
            ("Thumbs.db", b"junk"),
            (".hidden.png", b"junk"),
        ]);

        let files = vec![FileEntry {
            name: "test.cb7".into(),
            data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].name.contains("page1"));
    }

    #[test]
    fn no_prefix_when_basenames_unique_7z() {
        let png = tiny_png();
        let data = make_7z(&[("images/page01.png", &png), ("images/page02.png", &png)]);

        let files = vec![FileEntry {
            name: "manga.cb7".into(),
            data,
        }];

        let result = expand_archives(files).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].name.contains("page01"));
        assert!(result[1].name.contains("page02"));
        assert!(!result[0].name.contains("images"));
    }

    #[test]
    fn empty_7z_archive_returns_error() {
        let data = make_7z(&[("readme.txt", b"no images")]);

        let files = vec![FileEntry {
            name: "empty.cb7".into(),
            data,
        }];

        let err = expand_archives(files).unwrap_err();
        let chain = format!("{err:#}");
        assert!(
            chain.contains("no supported image files"),
            "unexpected error: {chain}",
        );
    }

    // ── archive export ──────────────────────────────────────────────

    #[test]
    fn create_cbz_produces_valid_zip() {
        let png = tiny_png();
        let entries = vec![
            ExportEntry {
                filename: "P001.webp".into(),
                data: png.clone(),
            },
            ExportEntry {
                filename: "P002.webp".into(),
                data: png.clone(),
            },
        ];

        let archive = create_archive(&entries, ArchiveFormat::Cbz).unwrap();
        assert!(is_zip(&archive));

        let cursor = Cursor::new(archive);
        let mut zip = zip::ZipArchive::new(cursor).unwrap();
        assert_eq!(zip.len(), 2);
        assert_eq!(zip.by_index(0).unwrap().name(), "P001.webp");
        assert_eq!(zip.by_index(1).unwrap().name(), "P002.webp");
    }

    #[test]
    fn create_cb7_produces_valid_7z() {
        let png = tiny_png();
        let entries = vec![ExportEntry {
            filename: "P001.webp".into(),
            data: png.clone(),
        }];

        let archive = create_archive(&entries, ArchiveFormat::Cb7).unwrap();
        assert!(is_7z(&archive));
    }

    #[test]
    fn create_cbz_roundtrips_content() {
        let png = tiny_png();
        let entries = vec![ExportEntry {
            filename: "P001.webp".into(),
            data: png.clone(),
        }];

        let archive = create_archive(&entries, ArchiveFormat::Cbz).unwrap();
        let cursor = Cursor::new(archive);
        let mut zip = zip::ZipArchive::new(cursor).unwrap();
        let mut buf = Vec::new();
        zip.by_index(0).unwrap().read_to_end(&mut buf).unwrap();
        assert_eq!(buf, png);
    }
}
