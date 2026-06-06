use std::path::Path;

use tokio::fs;
use tracing::{debug, info};

use crate::error::{Error, Result};

/// Archive output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    /// Standard ZIP
    Zip,
    /// Comic Book ZIP (same as ZIP with .cbz extension)
    Cbz,
}

/// Options for creating an archive.
#[derive(Debug, Clone)]
pub struct ArchiveOptions {
    /// Output format (Zip or Cbz)
    pub format: ArchiveFormat,
    /// Compression level (0-9, where 0 = store, 9 = best compression)
    pub compression_level: u8,
}

impl Default for ArchiveOptions {
    fn default() -> Self {
        Self {
            format: ArchiveFormat::Cbz,
            compression_level: 1, // Fast compression for images (already compressed)
        }
    }
}

/// A file entry to include in the archive.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Path to the source file on disk
    pub source_path: std::path::PathBuf,
    /// Name/path inside the archive (e.g. "001.jpg")
    pub archive_name: String,
}

/// Create a ZIP/CBZ archive from a list of files.
///
/// Files are written sequentially in a streaming fashion to avoid
/// loading everything into memory.
///
/// # Arguments
/// * `entries` - Ordered list of files to include
/// * `output_path` - Destination file path
/// * `options` - Archive options
pub async fn create_archive(
    entries: &[ArchiveEntry],
    output_path: &Path,
    options: &ArchiveOptions,
) -> Result<()> {
    info!(
        count = entries.len(),
        output = %output_path.display(),
        format = ?options.format,
        "creating archive"
    );

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // ZIP creation is synchronous (zip crate), so use spawn_blocking
    let entries = entries.to_vec();
    let output_path = output_path.to_path_buf();
    let compression_level = options.compression_level;

    tokio::task::spawn_blocking(move || {
        create_archive_sync(&entries, &output_path, compression_level)
    })
    .await
    .map_err(|e| Error::Zip(format!("task join error: {}", e)))?
}

/// Synchronous archive creation (runs in a blocking thread).
fn create_archive_sync(
    entries: &[ArchiveEntry],
    output_path: &Path,
    compression_level: u8,
) -> Result<()> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;
    use zip::CompressionMethod;
    use zip::ZipWriter;

    let file = std::fs::File::create(output_path).map_err(|e| Error::Zip(e.to_string()))?;
    let mut zip = ZipWriter::new(file);

    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(compression_level as i64));

    for (i, entry) in entries.iter().enumerate() {
        debug!(
            index = i,
            name = %entry.archive_name,
            source = %entry.source_path.display(),
            "adding file to archive"
        );

        // Read source file
        let mut source_file =
            std::fs::File::open(&entry.source_path).map_err(|e| Error::Zip(e.to_string()))?;

        let metadata = source_file
            .metadata()
            .map_err(|e| Error::Zip(e.to_string()))?;
        let file_size = metadata.len();

        zip.start_file(&entry.archive_name, options)
            .map_err(|e| Error::Zip(e.to_string()))?;

        // Stream file content in 64KB chunks to avoid memory explosion
        let mut buffer = vec![0u8; 64 * 1024];
        let mut remaining = file_size;
        while remaining > 0 {
            let to_read = std::cmp::min(remaining, buffer.len() as u64) as usize;
            let bytes_read = source_file
                .read(&mut buffer[..to_read])
                .map_err(|e| Error::Zip(e.to_string()))?;
            if bytes_read == 0 {
                break;
            }
            zip.write_all(&buffer[..bytes_read])
                .map_err(|e| Error::Zip(e.to_string()))?;
            remaining -= bytes_read as u64;
        }
    }

    zip.finish().map_err(|e| Error::Zip(e.to_string()))?;

    info!(
        output = %output_path.display(),
        "archive created successfully"
    );
    Ok(())
}

/// Build archive entries from a directory of downloaded files.
///
/// Scans the directory for image files, sorts them numerically,
/// and returns them as `ArchiveEntry` instances.
pub async fn entries_from_dir(
    dir: &Path,
    _archive_format: ArchiveFormat,
) -> Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();

    let mut read_dir = fs::read_dir(dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        let metadata = entry.metadata().await?;

        if !metadata.is_file() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();

        // Only include image files
        if name.ends_with(".jpg")
            || name.ends_with(".jpeg")
            || name.ends_with(".png")
            || name.ends_with(".gif")
        {
            entries.push(ArchiveEntry {
                source_path: path,
                archive_name: name,
            });
        }
    }

    // Sort by filename numerically (001.jpg, 002.jpg, ...)
    entries
        .sort_by(|a, b| natural_sort_key(&a.archive_name).cmp(&natural_sort_key(&b.archive_name)));

    Ok(entries)
}

/// Extract a numeric prefix from a filename for natural sorting.
fn natural_sort_key(name: &str) -> (u64, &str) {
    let num_end = name
        .chars()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(name.len());
    let num: u64 = name[..num_end].parse().unwrap_or(0);
    (num, name)
}
