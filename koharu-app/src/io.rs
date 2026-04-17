use std::path::Path;

use image::ImageFormat;
use koharu_core::Document;
use koharu_core::commands::{
    DeviceInfo, FileEntry, FileResult, OpenDocumentsPayload, OpenExternalPayload, ThumbnailResult,
};
use rfd::FileDialog;

use crate::AppResources;
use crate::utils::{encode_image_dynamic, mime_from_ext};

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

    let pages = state.storage.import_files(payload.files, true, None).await?;
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

    let _new_pages = state.storage.import_files(payload.files, false, None).await?;
    Ok(state.storage.page_count().await)
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

pub async fn import_from_paths(
    state: AppResources,
    paths: Vec<String>,
    insert_at: Option<usize>,
) -> anyhow::Result<usize> {
    if paths.is_empty() {
        anyhow::bail!("No file paths provided");
    }

    let files: Vec<FileEntry> = tokio::task::spawn_blocking(move || {
        let mut file_paths: Vec<std::path::PathBuf> = Vec::new();
        for raw_path in &paths {
            let path = Path::new(raw_path);
            if path.is_dir() {
                collect_images_from_dir(path, &mut file_paths);
            } else if is_allowed_image_path(path) {
                file_paths.push(path.to_path_buf());
            } else {
                tracing::debug!(path = %raw_path, "not a supported image extension");
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

    if files.is_empty() {
        anyhow::bail!("No valid image files found in the provided paths");
    }

    let imported = state
        .storage
        .import_files(files, false, insert_at)
        .await?;

    Ok(imported.len())
}
