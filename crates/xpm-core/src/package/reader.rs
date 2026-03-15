//! Archive reader for `.xp` and `.pkg.tar.zst` packages.
//!
//! Supports zstd (default), gzip, and xz compression. Auto-detects
//! compression from magic bytes.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use crate::error::XpmError;
use crate::package::buildinfo;
use crate::package::mtree;
use crate::package::pkginfo;
use crate::package::types::{BuildInfo, MtreeEntry, PackageFile, PackageMeta};

// ── Compression detection ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression {
    Zstd,
    Gzip,
    Xz,
    None,
}

fn detect_compression(path: &Path) -> Result<Compression, XpmError> {
    let mut file = File::open(path)?;
    let mut magic = [0u8; 6];
    let n = file.read(&mut magic)?;

    if n >= 4 && magic[..4] == [0x28, 0xB5, 0x2F, 0xFD] {
        return Ok(Compression::Zstd);
    }
    if n >= 2 && magic[..2] == [0x1F, 0x8B] {
        return Ok(Compression::Gzip);
    }
    if n >= 6 && magic[..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        return Ok(Compression::Xz);
    }

    Ok(Compression::None)
}

fn open_archive(path: &Path) -> Result<tar::Archive<Box<dyn Read>>, XpmError> {
    let file = File::open(path)?;
    let buf = BufReader::new(file);

    let reader: Box<dyn Read> = match detect_compression(path)? {
        Compression::Zstd => Box::new(zstd::Decoder::new(buf)?),
        Compression::Gzip => Box::new(flate2::read::GzDecoder::new(buf)),
        Compression::Xz => Box::new(xz2::read::XzDecoder::new(buf)),
        Compression::None => Box::new(buf),
    };

    Ok(tar::Archive::new(reader))
}

// ── Raw entry reader ──────────────────────────────────────────

/// Read a specific metadata entry from a package archive by name.
///
/// Returns `None` if the entry does not exist.
pub fn read_raw_entry(path: &Path, entry_name: &str) -> Result<Option<Vec<u8>>, XpmError> {
    let mut archive = open_archive(path)?;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.to_path_buf();
        let name = entry_path.to_string_lossy();

        // Match with or without leading "./"
        if name == entry_name || name == format!("./{entry_name}") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(Some(buf));
        }
    }

    Ok(None)
}

// ── Metadata reader ───────────────────────────────────────────

/// Parsed metadata from a package archive.
pub struct PackageMetadata {
    pub meta: PackageMeta,
    pub buildinfo: Option<BuildInfo>,
    pub mtree: Vec<MtreeEntry>,
}

/// Read and parse all metadata from a `.xp` or `.pkg.tar.zst` package.
pub fn read_metadata(path: &Path) -> Result<PackageMetadata, XpmError> {
    let mut archive = open_archive(path)?;

    let mut pkginfo_data: Option<Vec<u8>> = None;
    let mut buildinfo_data: Option<Vec<u8>> = None;
    let mut mtree_data: Option<Vec<u8>> = None;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.to_path_buf();
        let name = entry_path.to_string_lossy().to_string();

        let normalized = name.strip_prefix("./").unwrap_or(&name);

        match normalized {
            ".PKGINFO" => {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                pkginfo_data = Some(buf);
            }
            ".BUILDINFO" => {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                buildinfo_data = Some(buf);
            }
            ".MTREE" => {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                mtree_data = Some(buf);
            }
            _ => {}
        }
    }

    let pkginfo_bytes =
        pkginfo_data.ok_or_else(|| XpmError::Package("package missing .PKGINFO".into()))?;

    let meta = pkginfo::parse_pkginfo(&pkginfo_bytes)?;

    let buildinfo = match buildinfo_data {
        Some(data) => Some(buildinfo::parse_buildinfo(&data)?),
        None => None,
    };

    let mtree_entries = match mtree_data {
        Some(data) => mtree::parse_mtree(&data)?,
        None => Vec::new(),
    };

    Ok(PackageMetadata {
        meta,
        buildinfo,
        mtree: mtree_entries,
    })
}

// ── File listing ──────────────────────────────────────────────

/// List all non-metadata files inside a package archive.
pub fn list_files(path: &Path) -> Result<Vec<PackageFile>, XpmError> {
    let mut archive = open_archive(path)?;
    let mut files = Vec::new();

    for entry in archive.entries()? {
        let entry = entry?;
        let entry_path = entry.path()?.to_path_buf();
        let name = entry_path.to_string_lossy().to_string();

        let normalized = name.strip_prefix("./").unwrap_or(&name);

        // Skip metadata dot-files.
        if normalized.starts_with('.') {
            continue;
        }

        files.push(PackageFile {
            path: PathBuf::from(normalized),
            size: entry.size(),
            is_dir: entry.header().entry_type().is_dir(),
        });
    }

    Ok(files)
}

// ── Extract ───────────────────────────────────────────────────

/// Extract all package files (excluding metadata) to a destination directory.
pub fn extract_to(pkg_path: &Path, dest: &Path) -> Result<Vec<PathBuf>, XpmError> {
    let mut archive = open_archive(pkg_path)?;
    let mut extracted = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.to_path_buf();
        let name = entry_path.to_string_lossy().to_string();
        let normalized = name.strip_prefix("./").unwrap_or(&name);

        // Skip metadata dot-files.
        if normalized.starts_with('.') {
            continue;
        }

        let target = dest.join(normalized);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target)?;
            std::io::copy(&mut entry, &mut out)?;
        }
        extracted.push(PathBuf::from(normalized));
    }

    Ok(extracted)
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal .xp archive (tar.zst) in memory and write to a temp file.
    fn make_test_package(dir: &Path) -> PathBuf {
        let pkg_path = dir.join("test-1.0.0-1-x86_64.xp");

        let mut raw_tar = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut raw_tar);

            // .PKGINFO
            let pkginfo = b"pkgname = test\npkgver = 1.0.0-1\n\
                pkgdesc = A test package\narch = x86_64\nsize = 42\n";
            let mut header = tar::Header::new_gnu();
            header.set_path(".PKGINFO").unwrap();
            header.set_size(pkginfo.len() as u64);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            builder.append(&header, &pkginfo[..]).unwrap();

            // .BUILDINFO
            let buildinfo = b"pkgname = test\npkgver = 1.0.0-1\n\
                buildtool = xpkg 0.1.0\ncompress = zstd\n";
            let mut header = tar::Header::new_gnu();
            header.set_path(".BUILDINFO").unwrap();
            header.set_size(buildinfo.len() as u64);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            builder.append(&header, &buildinfo[..]).unwrap();

            // .MTREE
            let mtree = b"#mtree\n\
                ./usr type=dir mode=0755 uid=0 gid=0\n\
                ./usr/bin type=dir mode=0755 uid=0 gid=0\n\
                ./usr/bin/test type=file mode=0755 size=5 uid=0 gid=0\n";
            let mut header = tar::Header::new_gnu();
            header.set_path(".MTREE").unwrap();
            header.set_size(mtree.len() as u64);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            builder.append(&header, &mtree[..]).unwrap();

            // usr/ directory
            let mut header = tar::Header::new_gnu();
            header.set_path("usr/").unwrap();
            header.set_size(0);
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Directory);
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            builder.append(&header, &[][..]).unwrap();

            // usr/bin/ directory
            let mut header = tar::Header::new_gnu();
            header.set_path("usr/bin/").unwrap();
            header.set_size(0);
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Directory);
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            builder.append(&header, &[][..]).unwrap();

            // usr/bin/test file
            let content = b"hello";
            let mut header = tar::Header::new_gnu();
            header.set_path("usr/bin/test").unwrap();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            builder.append(&header, &content[..]).unwrap();

            builder.finish().unwrap();
        }

        // Compress with zstd.
        let compressed = zstd::encode_all(&raw_tar[..], 3).unwrap();
        let mut file = File::create(&pkg_path).unwrap();
        file.write_all(&compressed).unwrap();

        pkg_path
    }

    #[test]
    fn read_metadata_from_xp() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = make_test_package(dir.path());

        let md = read_metadata(&pkg).unwrap();
        assert_eq!(md.meta.name, "test");
        assert_eq!(md.meta.version, "1.0.0");
        assert_eq!(md.meta.release, "1");
        assert_eq!(md.meta.description, "A test package");
        assert!(md.buildinfo.is_some());
        assert_eq!(md.buildinfo.unwrap().buildtool, "xpkg 0.1.0");
        assert_eq!(md.mtree.len(), 3);
    }

    #[test]
    fn list_files_excludes_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = make_test_package(dir.path());

        let files = list_files(&pkg).unwrap();
        assert_eq!(files.len(), 3); // usr/, usr/bin/, usr/bin/test
        assert!(files
            .iter()
            .all(|f| !f.path.to_string_lossy().starts_with('.')));
    }

    #[test]
    fn extract_to_directory() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = make_test_package(dir.path());
        let dest = dir.path().join("extracted");
        std::fs::create_dir_all(&dest).unwrap();

        let extracted = extract_to(&pkg, &dest).unwrap();
        assert!(!extracted.is_empty());
        assert!(dest.join("usr/bin/test").exists());

        let content = std::fs::read_to_string(dest.join("usr/bin/test")).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn read_raw_entry_pkginfo() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = make_test_package(dir.path());

        let data = read_raw_entry(&pkg, ".PKGINFO").unwrap();
        assert!(data.is_some());
        let text = String::from_utf8(data.unwrap()).unwrap();
        assert!(text.contains("pkgname = test"));
    }

    #[test]
    fn read_raw_entry_missing() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = make_test_package(dir.path());

        let data = read_raw_entry(&pkg, ".INSTALL").unwrap();
        assert!(data.is_none());
    }

    #[test]
    fn detect_compression_zstd() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = make_test_package(dir.path());
        assert_eq!(detect_compression(&pkg).unwrap(), Compression::Zstd);
    }

    #[test]
    fn missing_pkginfo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_path = dir.path().join("bad.xp");

        // Create archive without .PKGINFO
        let mut raw_tar = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut raw_tar);
            let content = b"hello";
            let mut header = tar::Header::new_gnu();
            header.set_path("usr/bin/test").unwrap();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_uid(0);
            header.set_gid(0);
            header.set_cksum();
            builder.append(&header, &content[..]).unwrap();
            builder.finish().unwrap();
        }
        let compressed = zstd::encode_all(&raw_tar[..], 3).unwrap();
        std::fs::write(&pkg_path, compressed).unwrap();

        let result = read_metadata(&pkg_path);
        assert!(result.is_err());
    }

    #[test]
    fn nonexistent_file_returns_error() {
        let result = read_metadata(Path::new("/nonexistent/package.xp"));
        assert!(result.is_err());
    }
}
