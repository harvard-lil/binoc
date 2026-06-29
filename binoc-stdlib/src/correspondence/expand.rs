use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use binoc_sdk::{
    decompose_child, file_name, member_child, BinocError, BinocResult, DataAccess, Diagnostic,
    ExpandDescriptor, ExpandOutput, ExpandRule, ItemRef, NodeMatch, ProjectionHint,
};

/// Default decompression caps. These are bomb-defense bounds, not correctness
/// limits: they only need to sit comfortably above the largest real bundle we
/// expect to expand. The previous 256 MB / 512 MB values rejected real
/// multi-GB government bundles (e.g. the 3.25 GB USDA FoodData Central zip), so
/// the defaults are raised to GiB scale while staying finite. All three are
/// overridable per-dataset via [`CorrespondenceConfig`].
pub const DEFAULT_GZIP_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
pub const DEFAULT_ARCHIVE_MAX_ENTRY_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB
pub const DEFAULT_ARCHIVE_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024; // 8 GiB

/// Decompression size caps threaded into the archive/gzip expand rules. These
/// are the runtime values (after applying any per-dataset overrides over the
/// `DEFAULT_*` constants); see [`crate::correspondence::CorrespondenceOptions`].
#[derive(Debug, Clone, Copy)]
pub struct ExpandCaps {
    /// Max decompressed size of a single gzip stream.
    pub gzip_max_bytes: u64,
    /// Max decompressed size of one archive entry.
    pub archive_max_entry_bytes: u64,
    /// Max total decompressed size across a whole archive.
    pub archive_max_total_bytes: u64,
}

impl Default for ExpandCaps {
    fn default() -> Self {
        Self {
            gzip_max_bytes: DEFAULT_GZIP_MAX_BYTES,
            archive_max_entry_bytes: DEFAULT_ARCHIVE_MAX_ENTRY_BYTES,
            archive_max_total_bytes: DEFAULT_ARCHIVE_MAX_TOTAL_BYTES,
        }
    }
}

/// Human-readable name of the config knob that bounds a given cap, named in the
/// overflow error so a user knows exactly which value to raise.
const GZIP_CAP_KNOB: &str = "correspondence.max_gzip_bytes";
const ENTRY_CAP_KNOB: &str = "correspondence.max_archive_entry_bytes";
const TOTAL_CAP_KNOB: &str = "correspondence.max_archive_total_bytes";
const STREAM_COPY_BUFFER_BYTES: usize = 64 * 1024;

fn cap_overflow_message(what: &str, cap: u64, knob: &str) -> String {
    format!(
        "{what} exceeds the {cap}-byte decompression cap; raise `{knob}` in the dataset config to expand it"
    )
}

#[derive(Debug)]
enum StreamCopyError {
    SourceIo(std::io::Error),
    OutputIo(std::io::Error),
    Cap(String),
}

impl StreamCopyError {
    fn into_format_error(self, format: fn(String) -> BinocError) -> BinocError {
        match self {
            Self::SourceIo(err) => format(BinocError::Io(err).to_string()),
            Self::OutputIo(err) => BinocError::Io(err),
            Self::Cap(message) => format(message),
        }
    }
}

#[derive(Clone, Copy)]
struct StreamCap<'a> {
    cap: u64,
    message: &'a str,
}

struct StreamTotalCap<'a> {
    current: &'a mut u64,
    cap: u64,
    message: &'a str,
}

#[derive(Debug)]
struct StreamCopyStats {
    bytes_written: u64,
    hash: Option<String>,
}

fn stream_copy_with_caps<R: Read>(
    reader: &mut R,
    dest: &Path,
    entry_cap: StreamCap<'_>,
    mut total_cap: Option<StreamTotalCap<'_>>,
    hash_output: bool,
) -> Result<StreamCopyStats, StreamCopyError> {
    let result = (|| {
        let mut out = std::fs::File::create(dest).map_err(StreamCopyError::OutputIo)?;
        let mut buffer = [0u8; STREAM_COPY_BUFFER_BYTES];
        let mut entry_total = 0u64;
        let mut hasher = hash_output.then(blake3::Hasher::new);

        loop {
            let entry_remaining = entry_cap.cap.saturating_sub(entry_total);
            if entry_remaining == 0 {
                if read_over_cap_probe(reader)? {
                    return Err(StreamCopyError::Cap(entry_cap.message.to_string()));
                }
                break;
            }

            let total_remaining = total_cap
                .as_ref()
                .map(|total| total.cap.saturating_sub(*total.current))
                .unwrap_or(u64::MAX);
            if total_remaining == 0 {
                if read_over_cap_probe(reader)? {
                    let message = total_cap
                        .as_ref()
                        .expect("total cap must be present")
                        .message
                        .to_string();
                    return Err(StreamCopyError::Cap(message));
                }
                break;
            }

            let read_limit = STREAM_COPY_BUFFER_BYTES.min(
                entry_remaining
                    .min(total_remaining)
                    .try_into()
                    .unwrap_or(usize::MAX),
            );
            let bytes_read = reader
                .read(&mut buffer[..read_limit])
                .map_err(StreamCopyError::SourceIo)?;
            if bytes_read == 0 {
                break;
            }
            let bytes_read_u64 = bytes_read as u64;
            let new_entry_total = entry_total
                .checked_add(bytes_read_u64)
                .ok_or_else(|| StreamCopyError::Cap(entry_cap.message.to_string()))?;
            let new_total = total_cap
                .as_ref()
                .map(|total| {
                    (*total.current)
                        .checked_add(bytes_read_u64)
                        .ok_or_else(|| StreamCopyError::Cap(total.message.to_string()))
                })
                .transpose()?;

            out.write_all(&buffer[..bytes_read])
                .map_err(StreamCopyError::OutputIo)?;
            if let Some(hasher) = hasher.as_mut() {
                hasher.update(&buffer[..bytes_read]);
            }
            entry_total = new_entry_total;
            if let (Some(total), Some(new_total)) = (total_cap.as_mut(), new_total) {
                *total.current = new_total;
            }
        }

        out.flush().map_err(StreamCopyError::OutputIo)?;
        Ok(StreamCopyStats {
            bytes_written: entry_total,
            hash: hasher.map(|hasher| hasher.finalize().to_hex().to_string()),
        })
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

fn read_over_cap_probe<R: Read>(reader: &mut R) -> Result<bool, StreamCopyError> {
    let mut byte = [0u8; 1];
    reader
        .read(&mut byte)
        .map(|bytes_read| bytes_read > 0)
        .map_err(StreamCopyError::SourceIo)
}

/// Which separator the immediate children of an expansion hang off.
///
/// Directory traversal reveals an already-navigable tree, so its members use
/// `/`. An archive extraction is a decompose boundary (binoc opened a format to
/// reveal the members), so the immediate members use `/>`. Deeper nesting inside
/// an extracted tree is produced by [`DirectoryExpand`] re-firing on the dir
/// child nodes, which correctly uses `/`.
#[derive(Clone, Copy)]
enum ChildSep {
    Member,
    Decompose,
}

impl ChildSep {
    fn join(self, parent: &str, name: &str) -> String {
        match self {
            ChildSep::Member => member_child(parent, name),
            ChildSep::Decompose => decompose_child(parent, name),
        }
    }
}

fn projection_for(logical_path: &str, is_dir: bool) -> ProjectionHint {
    if is_dir {
        return ProjectionHint::default().item_type("directory");
    }
    match Path::new(logical_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv" | "tsv") => ProjectionHint::default().item_type("tabular"),
        Some("txt" | "md") => ProjectionHint::default().item_type("text"),
        _ => ProjectionHint::default().item_type("file"),
    }
}

fn make_child(physical: &Path, logical: String, data: &dyn DataAccess) -> BinocResult<ItemRef> {
    let mut item = data.register_local(physical, &logical)?;
    if !item.is_dir {
        let (hash, size) = hash_and_size(&item, data)?;
        item.content_hash = Some(hash);
        item.size = Some(size);
    }
    item.projection_hint = projection_for(&logical, item.is_dir);
    Ok(item)
}

fn hash_and_size(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<(String, u64)> {
    let mut reader = data.open_read(item)?;
    let mut hasher = blake3::Hasher::new();
    let size = std::io::copy(&mut reader, &mut hasher).map_err(BinocError::Io)?;
    Ok((hasher.finalize().to_hex().to_string(), size))
}

fn expand_physical_dir(
    dir: &Path,
    parent_logical: &str,
    sep: ChildSep,
    data: &dyn DataAccess,
) -> BinocResult<Vec<ItemRef>> {
    let mut children = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .min_depth(1)
        .max_depth(1)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|err| BinocError::Io(err.into()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        children.push(make_child(
            entry.path(),
            sep.join(parent_logical, &name),
            data,
        )?);
    }
    Ok(children)
}

pub struct DirectoryExpand;

impl ExpandRule for DirectoryExpand {
    fn descriptor(&self) -> ExpandDescriptor {
        ExpandDescriptor {
            name: "binoc.expand.directory".into(),
            input: NodeMatch {
                is_dir: Some(true),
                ..NodeMatch::default()
            },
            fires_beneath_settled: false,
        }
    }

    fn expand(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ExpandOutput> {
        let physical = data.local_path(item)?;
        expand_physical_dir(&physical, &item.logical_path, ChildSep::Member, data).map(Into::into)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ZipExpand {
    pub caps: ExpandCaps,
}

impl ExpandRule for ZipExpand {
    fn descriptor(&self) -> ExpandDescriptor {
        ExpandDescriptor {
            name: "binoc.expand.zip".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".zip".into()],
                ..NodeMatch::default()
            },
            fires_beneath_settled: false,
        }
    }

    fn expand(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ExpandOutput> {
        let physical = data.local_path(item)?;
        let workspace = data.workspace()?;
        let diagnostics = extract_zip(&physical, &workspace, self.caps)?;
        let children =
            expand_physical_dir(&workspace, &item.logical_path, ChildSep::Decompose, data)?;
        Ok(ExpandOutput {
            children,
            diagnostics,
        })
    }
}

fn extract_zip(zip_path: &Path, dest: &Path, caps: ExpandCaps) -> BinocResult<Vec<Diagnostic>> {
    let file = std::fs::File::open(zip_path).map_err(BinocError::Io)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| BinocError::Zip(err.to_string()))?;
    let mut total = 0u64;
    let mut diagnostics = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| BinocError::Zip(err.to_string()))?;
        let Some(entry_path) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            diagnostics.push(
                Diagnostic::warning(
                    "binoc.archive_entry_skipped",
                    "Skipped unsafe zip entry path",
                )
                .with_location(entry.name().to_string()),
            );
            continue;
        };
        let out_path = dest.join(entry_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(BinocError::Io)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(BinocError::Io)?;
        }
        stream_copy_with_caps(
            &mut entry,
            &out_path,
            StreamCap {
                cap: caps.archive_max_entry_bytes,
                message: &cap_overflow_message(
                    "zip entry",
                    caps.archive_max_entry_bytes,
                    ENTRY_CAP_KNOB,
                ),
            },
            Some(StreamTotalCap {
                current: &mut total,
                cap: caps.archive_max_total_bytes,
                message: &cap_overflow_message(
                    "zip total output",
                    caps.archive_max_total_bytes,
                    TOTAL_CAP_KNOB,
                ),
            }),
            false,
        )
        .map_err(|err| err.into_format_error(BinocError::Zip))?;
    }
    Ok(diagnostics)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TarExpand {
    pub caps: ExpandCaps,
}

impl ExpandRule for TarExpand {
    fn descriptor(&self) -> ExpandDescriptor {
        ExpandDescriptor {
            name: "binoc.expand.tar".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".tar".into(), ".tgz".into()],
                ..NodeMatch::default()
            },
            fires_beneath_settled: false,
        }
    }

    fn expand(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ExpandOutput> {
        let physical = data.local_path(item)?;
        let workspace = data.workspace()?;
        let file = std::fs::File::open(&physical).map_err(BinocError::Io)?;
        let diagnostics =
            if item.logical_path.ends_with(".tgz") || item.logical_path.ends_with(".tar.gz") {
                extract_tar(flate2::read::GzDecoder::new(file), &workspace, self.caps)?
            } else {
                extract_tar(file, &workspace, self.caps)?
            };
        let children =
            expand_physical_dir(&workspace, &item.logical_path, ChildSep::Decompose, data)?;
        Ok(ExpandOutput {
            children,
            diagnostics,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GzipExpand {
    pub caps: ExpandCaps,
}

impl ExpandRule for GzipExpand {
    fn descriptor(&self) -> ExpandDescriptor {
        ExpandDescriptor {
            name: "binoc.expand.gzip".into(),
            input: NodeMatch {
                is_dir: Some(false),
                extensions: vec![".gz".into()],
                ..NodeMatch::default()
            },
            fires_beneath_settled: false,
        }
    }

    fn expand(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ExpandOutput> {
        let name = file_name(&item.logical_path);
        let Some(inner_name) = name.strip_suffix(".gz") else {
            return Ok(Vec::new().into());
        };
        if inner_name.ends_with(".tar") {
            return TarExpand { caps: self.caps }.expand(item, data);
        }
        let workspace = data.workspace()?;
        let physical = workspace.join(inner_name);
        let mut reader = data.open_read(item)?;
        let mut decoder = flate2::read::GzDecoder::new(&mut reader);
        let stats = stream_copy_with_caps(
            &mut decoder,
            &physical,
            StreamCap {
                cap: self.caps.gzip_max_bytes,
                message: &cap_overflow_message(
                    "gzip output",
                    self.caps.gzip_max_bytes,
                    GZIP_CAP_KNOB,
                ),
            },
            None,
            true,
        )
        .map_err(|err| err.into_format_error(BinocError::Gzip))?;
        let logical = decompose_child(&item.logical_path, inner_name);
        let mut child = data.register_local(&physical, &logical)?;
        child.content_hash = stats.hash;
        child.size = Some(stats.bytes_written);
        child.projection_hint = projection_for(&logical, child.is_dir);
        Ok(vec![child].into())
    }
}

fn safe_archive_path(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn extract_tar<R: Read>(reader: R, dest: &Path, caps: ExpandCaps) -> BinocResult<Vec<Diagnostic>> {
    let mut archive = tar::Archive::new(reader);
    let mut total = 0u64;
    let mut diagnostics = Vec::new();
    for entry in archive
        .entries()
        .map_err(|err| BinocError::Tar(err.to_string()))?
    {
        let mut entry = entry.map_err(|err| BinocError::Tar(err.to_string()))?;
        let path = entry
            .path()
            .map_err(|err| BinocError::Tar(err.to_string()))?;
        let Some(path) = safe_archive_path(&path) else {
            diagnostics.push(
                Diagnostic::warning(
                    "binoc.archive_entry_skipped",
                    "Skipped unsafe tar entry path",
                )
                .with_location(path.to_string_lossy().to_string()),
            );
            continue;
        };
        let out_path = dest.join(path);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path).map_err(BinocError::Io)?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(BinocError::Io)?;
        }
        stream_copy_with_caps(
            &mut entry,
            &out_path,
            StreamCap {
                cap: caps.archive_max_entry_bytes,
                message: &cap_overflow_message(
                    "tar entry",
                    caps.archive_max_entry_bytes,
                    ENTRY_CAP_KNOB,
                ),
            },
            Some(StreamTotalCap {
                current: &mut total,
                cap: caps.archive_max_total_bytes,
                message: &cap_overflow_message(
                    "tar total output",
                    caps.archive_max_total_bytes,
                    TOTAL_CAP_KNOB,
                ),
            }),
            false,
        )
        .map_err(|err| err.into_format_error(BinocError::Tar))?;
    }
    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn stream_copy_writes_in_cap_output_and_hashes_incrementally() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out_path = temp.path().join("out.txt");
        let payload = b"stream me";
        let mut reader = Cursor::new(payload);
        let mut total = 7u64;

        let stats = stream_copy_with_caps(
            &mut reader,
            &out_path,
            StreamCap {
                cap: 1024,
                message: "entry cap",
            },
            Some(StreamTotalCap {
                current: &mut total,
                cap: 1024,
                message: "total cap",
            }),
            true,
        )
        .expect("copy succeeds");

        assert_eq!(std::fs::read(&out_path).expect("output"), payload);
        assert_eq!(stats.bytes_written, payload.len() as u64);
        assert_eq!(total, 7 + payload.len() as u64);
        assert_eq!(
            stats.hash.as_deref(),
            Some(blake3::hash(payload).to_hex().as_str())
        );
    }

    #[test]
    fn stream_copy_removes_partial_file_on_entry_cap() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out_path = temp.path().join("out.txt");
        let mut reader = Cursor::new(b"abcdef");
        let mut total = 0u64;

        let err = stream_copy_with_caps(
            &mut reader,
            &out_path,
            StreamCap {
                cap: 3,
                message: "entry cap",
            },
            Some(StreamTotalCap {
                current: &mut total,
                cap: 1024,
                message: "total cap",
            }),
            false,
        )
        .expect_err("entry cap should fail");

        assert!(matches!(err, StreamCopyError::Cap(message) if message == "entry cap"));
        assert!(!out_path.exists(), "partial output should be removed");
        assert_eq!(total, 3);
    }

    #[test]
    fn stream_copy_removes_partial_file_on_total_cap() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out_path = temp.path().join("out.txt");
        let mut reader = Cursor::new(b"abcdef");
        let mut total = 2u64;

        let err = stream_copy_with_caps(
            &mut reader,
            &out_path,
            StreamCap {
                cap: 1024,
                message: "entry cap",
            },
            Some(StreamTotalCap {
                current: &mut total,
                cap: 5,
                message: "total cap",
            }),
            false,
        )
        .expect_err("total cap should fail");

        assert!(matches!(err, StreamCopyError::Cap(message) if message == "total cap"));
        assert!(!out_path.exists(), "partial output should be removed");
        assert_eq!(total, 5);
    }
}
