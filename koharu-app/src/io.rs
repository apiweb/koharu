use std::path::Path;

use image::ImageFormat;
use koharu_core::Document;
use koharu_core::commands::{
    DeviceInfo, FileEntry, FileResult, OpenDocumentsPayload, OpenExternalPayload, ThumbnailResult,
};
use rfd::FileDialog;

use crate::AppResources;
use crate::utils::{encode_image_dynamic, mime_from_ext};

/// Allowed image file extensions for import (lowercase, with leading dot).
const ALLOWED_IMAGE_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".webp"];

pub async fn app_version(state: AppResources) -> anyhow::Result<String> {
    Ok(state.version.to_string())
}

pub async fn device(state: AppResources) -> anyhow::Result<DeviceInfo> {
    Ok(DeviceInfo {
        ml_device: match state.device {
            koharu_ml::Device::Cpu => "CPU".to_string(),
            koharu_ml::Device::Cuda(_) => "CUDA".to_string(),
            koharu_ml::Device::Metal(_) => "Metal".to_string(),
        },
    })
}

pub async fn open_external(
    _state: AppResources,
    payload: OpenExternalPayload,
) -> anyhow::Result<()> {
    open::that(&payload.url)?;
    Ok(())
}

pub async fn get_documents(state: AppResources) -> anyhow::Result<usize> {
    Ok(state.storage.page_count().await)
}

// list_documents is now async — callers use storage.list_pages() directly

pub async fn get_document(state: AppResources, document_id: &str) -> anyhow::Result<Document> {
    state.storage.page(document_id).await
}

pub async fn get_thumbnail(
    state: AppResources,
    document_id: &str,
) -> anyhow::Result<ThumbnailResult> {
    let doc = state.storage.page(document_id).await?;

    let source_ref = doc.rendered.as_ref().unwrap_or(&doc.source);
    let source_img = state.storage.images.load(source_ref)?;
    let thumbnail = source_img.thumbnail(200, 200);

    let mut buf = std::io::Cursor::new(Vec::new());
    thumbnail.write_to(&mut buf, ImageFormat::WebP)?;

    Ok(ThumbnailResult {
        data: buf.into_inner(),
        content_type: "image/webp".to_string(),
    })
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn open_documents(
    state: AppResources,
    payload: OpenDocumentsPayload,
) -> anyhow::Result<usize> {
    if payload.files.is_empty() {
        anyhow::bail!("No files uploaded");
    }

    let pages = state
        .storage
        .import_files(payload.files, true, None)
        .await?;
    Ok(pages.len())
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn add_documents(
    state: AppResources,
    payload: OpenDocumentsPayload,
) -> anyhow::Result<usize> {
    if payload.files.is_empty() {
        anyhow::bail!("No files uploaded");
    }

    let _new_pages = state
        .storage
        .import_files(payload.files, false, None)
        .await?;
    Ok(state.storage.page_count().await)
}

/// Check whether a file path has an allowed image extension.
fn is_allowed_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let lower = ext.to_ascii_lowercase();
            ALLOWED_IMAGE_EXTENSIONS
                .iter()
                .any(|allowed| allowed.trim_start_matches('.') == lower)
        })
}

/// Recursively collect image file paths from a directory.
fn collect_images_from_dir(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!(path = %dir.display(), error = %err, "failed to read directory");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_images_from_dir(&path, out);
        } else if is_allowed_image_path(&path) {
            out.push(path);
        }
    }
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Maximum body size when fetching an image from a URL (50 MiB).
const MAX_URL_FETCH_BYTES: usize = 50 * 1024 * 1024;

async fn fetch_image_from_url(
    client: &reqwest_middleware::ClientWithMiddleware,
    url: &str,
) -> anyhow::Result<FileEntry> {
    let response = client.get(url).send().await?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {status} fetching image");
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if !content_type.is_empty() && !content_type.starts_with("image/") {
        anyhow::bail!("response Content-Type is not an image: {content_type}");
    }

    let content_length = response.content_length().unwrap_or(0) as usize;
    if content_length > MAX_URL_FETCH_BYTES {
        anyhow::bail!("image too large ({content_length} bytes, max {MAX_URL_FETCH_BYTES})");
    }

    let data = response.bytes().await?.to_vec();
    if data.len() > MAX_URL_FETCH_BYTES {
        anyhow::bail!(
            "image too large ({} bytes, max {MAX_URL_FETCH_BYTES})",
            data.len()
        );
    }

    // Derive a filename from the URL path, falling back to a generic name.
    let name = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.path_segments()?
                .rev()
                .find(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "image".to_string());

    Ok(FileEntry { name, data })
}

/// Import images from absolute file-system paths (and/or HTTP(S) URLs).
///
/// Directories are recursively scanned for image files. Only files whose
/// extension matches [`ALLOWED_IMAGE_EXTENSIONS`] are read from directories.
/// Individual file paths are always read regardless of extension — the image
/// decoder in [`Storage::import_files`] rejects non-image content.
///
/// Entries that look like HTTP(S) URLs are fetched over the network using the
/// shared HTTP client. A 50 MiB per-URL size limit is enforced.
#[tracing::instrument(level = "info", skip_all)]
pub async fn import_from_paths(
    state: AppResources,
    paths: Vec<String>,
    replace: bool,
    insert_at: Option<usize>,
) -> anyhow::Result<usize> {
    if paths.is_empty() {
        anyhow::bail!("No file paths provided");
    }

    let (url_paths, local_paths): (Vec<String>, Vec<String>) =
        paths.into_iter().partition(|p| is_url(p));

    // Fetch URL images asynchronously.
    let mut url_files: Vec<FileEntry> = Vec::new();
    if !url_paths.is_empty() {
        let client = state.runtime.http_client();
        for url in &url_paths {
            match fetch_image_from_url(&client, url).await {
                Ok(entry) => url_files.push(entry),
                Err(err) => {
                    tracing::warn!(url = %url, error = %err, "failed to fetch image from URL");
                }
            }
        }
    }

    // Read local files on a blocking thread.
    let local_files: Vec<FileEntry> = tokio::task::spawn_blocking(move || {
        let mut file_paths: Vec<std::path::PathBuf> = Vec::new();
        for raw_path in &local_paths {
            let path = Path::new(raw_path);
            if path.is_dir() {
                collect_images_from_dir(path, &mut file_paths);
            } else if is_allowed_image_path(path) {
                file_paths.push(path.to_path_buf());
            } else if path.is_file() {
                // Explicitly-dropped files (e.g. browser DnD temp files) may
                // lack a standard image extension. Read them anyway — the
                // image decoder will reject non-image content.
                tracing::debug!(path = %raw_path, "non-standard extension, will try decoding");
                file_paths.push(path.to_path_buf());
            } else {
                tracing::warn!(path = %raw_path, "skipped non-image file during drag-drop import");
            }
        }

        let mut entries = Vec::new();
        for path in &file_paths {
            match std::fs::read(path) {
                Ok(data) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "unknown".to_string());
                    entries.push(FileEntry { name, data });
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "failed to read file");
                }
            }
        }
        entries
    })
    .await?;

    let mut files = url_files;
    files.extend(local_files);

    if files.is_empty() {
        anyhow::bail!("No valid image files found in the provided paths");
    }

    let count = files.len();
    state
        .storage
        .import_files(files, replace, insert_at)
        .await?;
    Ok(count)
}

#[tracing::instrument(level = "info", skip_all)]
pub async fn export_document(state: AppResources, document_id: &str) -> anyhow::Result<FileResult> {
    let doc = state.storage.page(document_id).await?;

    let rendered_ref = doc
        .rendered
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No rendered image found"))?;
    let rendered_img = state.storage.images.load(rendered_ref)?;

    let ext = "webp";
    let bytes = encode_image_dynamic(&rendered_img, ext)?;
    let filename = format!("{}_koharu.{}", doc.name, ext);
    let content_type = mime_from_ext(ext).to_string();

    Ok(FileResult {
        filename,
        data: bytes,
        content_type,
    })
}

pub async fn export_all_inpainted(state: AppResources) -> anyhow::Result<usize> {
    let Some(output_dir) = pick_output_dir().await? else {
        return Ok(0);
    };

    let pages = state.storage.with_project(|p| p.pages.clone()).await;
    let mut exported = 0usize;
    for doc in &pages {
        let Some(ref inpainted_ref) = doc.inpainted else {
            continue;
        };
        let img = state.storage.images.load(inpainted_ref)?;
        let output_path = output_dir.join(format!("{}_inpainted.webp", doc.name));
        let bytes = encode_image_dynamic(&img, "webp")?;
        std::fs::write(&output_path, bytes)?;
        exported += 1;
    }
    anyhow::ensure!(exported > 0, "No inpainted images found to export");
    Ok(exported)
}

pub async fn export_all_rendered(state: AppResources) -> anyhow::Result<usize> {
    let Some(output_dir) = pick_output_dir().await? else {
        return Ok(0);
    };

    let pages = state.storage.with_project(|p| p.pages.clone()).await;
    let mut exported = 0usize;
    for doc in &pages {
        let Some(ref rendered_ref) = doc.rendered else {
            continue;
        };
        let img = state.storage.images.load(rendered_ref)?;
        let output_path = output_dir.join(format!("{}_rendered.webp", doc.name));
        let bytes = encode_image_dynamic(&img, "webp")?;
        std::fs::write(&output_path, bytes)?;
        exported += 1;
    }
    anyhow::ensure!(exported > 0, "No rendered images found to export");
    Ok(exported)
}

async fn pick_output_dir() -> anyhow::Result<Option<std::path::PathBuf>> {
    Ok(tokio::task::spawn_blocking(|| FileDialog::new().pick_folder()).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_allowed_image_path_accepts_png() {
        assert!(is_allowed_image_path(Path::new("/tmp/photo.png")));
    }

    #[test]
    fn is_allowed_image_path_accepts_jpg() {
        assert!(is_allowed_image_path(Path::new("/tmp/photo.jpg")));
    }

    #[test]
    fn is_allowed_image_path_accepts_jpeg() {
        assert!(is_allowed_image_path(Path::new("/tmp/photo.jpeg")));
    }

    #[test]
    fn is_allowed_image_path_accepts_webp() {
        assert!(is_allowed_image_path(Path::new("/tmp/photo.webp")));
    }

    #[test]
    fn is_allowed_image_path_case_insensitive() {
        assert!(is_allowed_image_path(Path::new("/tmp/photo.PNG")));
        assert!(is_allowed_image_path(Path::new("/tmp/photo.Jpg")));
        assert!(is_allowed_image_path(Path::new("/tmp/photo.WEBP")));
    }

    #[test]
    fn is_allowed_image_path_rejects_non_images() {
        assert!(!is_allowed_image_path(Path::new("/tmp/file.txt")));
        assert!(!is_allowed_image_path(Path::new("/tmp/doc.pdf")));
        assert!(!is_allowed_image_path(Path::new("/tmp/script.js")));
        assert!(!is_allowed_image_path(Path::new("/tmp/style.css")));
        assert!(!is_allowed_image_path(Path::new("/tmp/archive.zip")));
        assert!(!is_allowed_image_path(Path::new("/tmp/data.cbz")));
    }

    #[test]
    fn is_allowed_image_path_rejects_no_extension() {
        assert!(!is_allowed_image_path(Path::new("/tmp/noext")));
    }

    #[test]
    fn is_allowed_image_path_rejects_hidden_files() {
        assert!(!is_allowed_image_path(Path::new("/tmp/.DS_Store")));
    }

    #[test]
    fn is_allowed_image_path_rejects_gif_and_bmp() {
        assert!(!is_allowed_image_path(Path::new("/tmp/anim.gif")));
        assert!(!is_allowed_image_path(Path::new("/tmp/photo.bmp")));
    }

    #[test]
    fn collect_images_from_dir_finds_nested_images() {
        let dir = std::env::temp_dir().join("koharu_test_collect_images");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();

        std::fs::write(dir.join("a.png"), b"fake").unwrap();
        std::fs::write(dir.join("b.txt"), b"skip").unwrap();
        std::fs::write(dir.join("sub/c.jpg"), b"fake").unwrap();
        std::fs::write(dir.join("sub/d.webp"), b"fake").unwrap();
        std::fs::write(dir.join("sub/.DS_Store"), b"skip").unwrap();

        let mut found = Vec::new();
        collect_images_from_dir(&dir, &mut found);

        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert!(names.contains(&"a.png".to_string()));
        assert!(names.contains(&"c.jpg".to_string()));
        assert!(names.contains(&"d.webp".to_string()));
        assert!(!names.contains(&"b.txt".to_string()));
        assert!(!names.contains(&".DS_Store".to_string()));
        assert_eq!(found.len(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_images_from_dir_empty_dir_returns_nothing() {
        let dir = std::env::temp_dir().join("koharu_test_empty_dir");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut found = Vec::new();
        collect_images_from_dir(&dir, &mut found);
        assert!(found.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_images_from_dir_nonexistent_dir_is_noop() {
        let dir = Path::new("/tmp/koharu_test_nonexistent_dir_xyz");
        let mut found = Vec::new();
        collect_images_from_dir(dir, &mut found);
        assert!(found.is_empty());
    }
}
