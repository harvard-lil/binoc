use std::io::Read;
use std::path::{Component, Path, PathBuf};

use binoc_sdk::{
    decompose_child, file_name, member_child, BinocError, BinocResult, DataAccess, Diagnostic,
    ExpandDescriptor, ExpandOutput, ExpandRule, ItemRef, NodeMatch, ProjectionHint,
};

const GZIP_MAX_BYTES: u64 = 256 * 1024 * 1024;
const ARCHIVE_MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const ARCHIVE_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

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

pub struct ZipExpand;

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
        let diagnostics = extract_zip(&physical, &workspace)?;
        let children =
            expand_physical_dir(&workspace, &item.logical_path, ChildSep::Decompose, data)?;
        Ok(ExpandOutput {
            children,
            diagnostics,
        })
    }
}

fn extract_zip(zip_path: &Path, dest: &Path) -> BinocResult<Vec<Diagnostic>> {
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
        let buffer = read_capped(
            &mut entry,
            ARCHIVE_MAX_ENTRY_BYTES,
            "zip entry exceeds decompression cap",
        )
        .map_err(|err| BinocError::Zip(err.to_string()))?;
        total = total
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| BinocError::Zip("zip output size overflow".into()))?;
        if total > ARCHIVE_MAX_TOTAL_BYTES {
            return Err(BinocError::Zip(
                "zip total output exceeds decompression cap".into(),
            ));
        }
        std::fs::write(&out_path, &buffer).map_err(BinocError::Io)?;
    }
    Ok(diagnostics)
}

pub struct TarExpand;

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
                extract_tar(flate2::read::GzDecoder::new(file), &workspace)?
            } else {
                extract_tar(file, &workspace)?
            };
        let children =
            expand_physical_dir(&workspace, &item.logical_path, ChildSep::Decompose, data)?;
        Ok(ExpandOutput {
            children,
            diagnostics,
        })
    }
}

pub struct GzipExpand;

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
            return TarExpand.expand(item, data);
        }
        let bytes = data.read_bytes(item)?;
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let output = read_capped(
            &mut decoder,
            GZIP_MAX_BYTES,
            "gzip output exceeds decompression cap",
        )
        .map_err(|err| BinocError::Gzip(err.to_string()))?;
        let workspace = data.workspace()?;
        let physical = workspace.join(inner_name);
        std::fs::write(&physical, &output).map_err(BinocError::Io)?;
        let logical = decompose_child(&item.logical_path, inner_name);
        let mut child = data.register_local(&physical, &logical)?;
        child.content_hash = Some(blake3::hash(&output).to_hex().to_string());
        child.size = Some(output.len() as u64);
        child.projection_hint = projection_for(&logical, child.is_dir);
        Ok(vec![child].into())
    }
}

fn read_capped<R: Read>(reader: &mut R, cap: u64, message: &str) -> BinocResult<Vec<u8>> {
    let mut out = Vec::new();
    reader
        .take(cap + 1)
        .read_to_end(&mut out)
        .map_err(BinocError::Io)?;
    if out.len() as u64 > cap {
        return Err(BinocError::Other(message.into()));
    }
    Ok(out)
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

fn extract_tar<R: Read>(reader: R, dest: &Path) -> BinocResult<Vec<Diagnostic>> {
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
        let buffer = read_capped(
            &mut entry,
            ARCHIVE_MAX_ENTRY_BYTES,
            "tar entry exceeds decompression cap",
        )
        .map_err(|err| BinocError::Tar(err.to_string()))?;
        total = total
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| BinocError::Tar("tar output size overflow".into()))?;
        if total > ARCHIVE_MAX_TOTAL_BYTES {
            return Err(BinocError::Tar(
                "tar total output exceeds decompression cap".into(),
            ));
        }
        std::fs::write(&out_path, &buffer).map_err(BinocError::Io)?;
    }
    Ok(diagnostics)
}
