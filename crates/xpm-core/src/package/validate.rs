//! Post-extraction integrity validation against `.MTREE` manifests.
//!
//! Verifies that extracted files match the expected SHA-256 checksums,
//! sizes, and types recorded in the package's `.MTREE`.

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::XpmError;
use crate::package::types::{MtreeEntry, MtreeFileType};

// ── Validation result ─────────────────────────────────────────

/// A single integrity check failure.
#[derive(Debug, Clone)]
pub struct IntegrityError {
    pub path: String,
    pub kind: IntegrityErrorKind,
}

/// What went wrong during integrity checking.
#[derive(Debug, Clone)]
pub enum IntegrityErrorKind {
    Missing,
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
    SizeMismatch {
        expected: u64,
        actual: u64,
    },
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = &self.path;
        match &self.kind {
            IntegrityErrorKind::Missing => write!(f, "{path}: missing"),
            IntegrityErrorKind::TypeMismatch { expected, actual } => {
                write!(f, "{path}: expected {expected}, found {actual}")
            }
            IntegrityErrorKind::SizeMismatch { expected, actual } => {
                write!(f, "{path}: size {actual} != expected {expected}")
            }
            IntegrityErrorKind::ChecksumMismatch { expected, actual } => {
                write!(f, "{path}: sha256 {actual} != expected {expected}")
            }
        }
    }
}

// ── Public API ────────────────────────────────────────────────

/// Validate extracted files against an MTREE manifest.
///
/// `root` is the directory where files were extracted (e.g. `/` or a staging dir).
/// Returns a list of integrity errors (empty = all OK).
pub fn validate_integrity(
    root: &Path,
    entries: &[MtreeEntry],
) -> Result<Vec<IntegrityError>, XpmError> {
    let mut errors = Vec::new();

    for entry in entries {
        // Strip leading "./" from mtree path.
        let rel = entry
            .path
            .to_string_lossy()
            .strip_prefix("./")
            .unwrap_or(&entry.path.to_string_lossy())
            .to_string();

        let full_path = root.join(&rel);

        // Check existence.
        if !full_path.exists() && !full_path.is_symlink() {
            errors.push(IntegrityError {
                path: rel,
                kind: IntegrityErrorKind::Missing,
            });
            continue;
        }

        // Check type.
        match entry.file_type {
            MtreeFileType::Dir => {
                if !full_path.is_dir() {
                    errors.push(IntegrityError {
                        path: rel,
                        kind: IntegrityErrorKind::TypeMismatch {
                            expected: "dir",
                            actual: if full_path.is_file() { "file" } else { "other" },
                        },
                    });
                    continue;
                }
            }
            MtreeFileType::Link => {
                if !full_path.is_symlink() {
                    errors.push(IntegrityError {
                        path: rel,
                        kind: IntegrityErrorKind::TypeMismatch {
                            expected: "link",
                            actual: if full_path.is_file() { "file" } else { "other" },
                        },
                    });
                    continue;
                }
            }
            MtreeFileType::File => {
                if !full_path.is_file() {
                    errors.push(IntegrityError {
                        path: rel,
                        kind: IntegrityErrorKind::TypeMismatch {
                            expected: "file",
                            actual: if full_path.is_dir() { "dir" } else { "other" },
                        },
                    });
                    continue;
                }

                // Check size.
                if entry.size > 0 {
                    let actual_size = fs::metadata(&full_path)?.len();
                    if actual_size != entry.size {
                        errors.push(IntegrityError {
                            path: rel.clone(),
                            kind: IntegrityErrorKind::SizeMismatch {
                                expected: entry.size,
                                actual: actual_size,
                            },
                        });
                    }
                }

                // Check SHA-256.
                if let Some(expected_hash) = &entry.sha256 {
                    let actual_hash = sha256_file(&full_path)?;
                    if actual_hash != *expected_hash {
                        errors.push(IntegrityError {
                            path: rel,
                            kind: IntegrityErrorKind::ChecksumMismatch {
                                expected: expected_hash.clone(),
                                actual: actual_hash,
                            },
                        });
                    }
                }
            }
        }
    }

    Ok(errors)
}

// ── Helpers ───────────────────────────────────────────────────

fn sha256_file(path: &Path) -> Result<String, XpmError> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs as unix_fs;
    use std::path::PathBuf;

    fn make_entry(path: &str, ft: MtreeFileType, size: u64, sha256: Option<&str>) -> MtreeEntry {
        MtreeEntry {
            path: PathBuf::from(path),
            file_type: ft,
            mode: 0o644,
            uid: 0,
            gid: 0,
            size,
            sha256: sha256.map(String::from),
            link_target: None,
        }
    }

    #[test]
    fn validate_correct_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a file.
        fs::create_dir_all(root.join("usr/bin")).unwrap();
        fs::write(root.join("usr/bin/hello"), "hello").unwrap();

        let hash = sha256_file(&root.join("usr/bin/hello")).unwrap();

        let entries = vec![
            make_entry("./usr", MtreeFileType::Dir, 0, None),
            make_entry("./usr/bin", MtreeFileType::Dir, 0, None),
            make_entry("./usr/bin/hello", MtreeFileType::File, 5, Some(&hash)),
        ];

        let errors = validate_integrity(root, &entries).unwrap();
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    #[test]
    fn detect_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let entries = vec![make_entry(
            "./usr/bin/missing",
            MtreeFileType::File,
            0,
            None,
        )];

        let errors = validate_integrity(dir.path(), &entries).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0].kind, IntegrityErrorKind::Missing));
    }

    #[test]
    fn detect_type_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("target"), "data").unwrap();

        let entries = vec![make_entry("./target", MtreeFileType::Dir, 0, None)];

        let errors = validate_integrity(dir.path(), &entries).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            IntegrityErrorKind::TypeMismatch { .. }
        ));
    }

    #[test]
    fn detect_size_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("f"), "abc").unwrap();

        let entries = vec![make_entry("./f", MtreeFileType::File, 999, None)];

        let errors = validate_integrity(dir.path(), &entries).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            IntegrityErrorKind::SizeMismatch { .. }
        ));
    }

    #[test]
    fn detect_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("f"), "abc").unwrap();

        let entries = vec![make_entry(
            "./f",
            MtreeFileType::File,
            3,
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
        )];

        let errors = validate_integrity(dir.path(), &entries).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors[0].kind,
            IntegrityErrorKind::ChecksumMismatch { .. }
        ));
    }

    #[test]
    fn validate_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("target"), "data").unwrap();
        unix_fs::symlink("target", root.join("link")).unwrap();

        let mut entry = make_entry("./link", MtreeFileType::Link, 0, None);
        entry.link_target = Some("target".into());

        let errors = validate_integrity(root, &[entry]).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_empty_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let errors = validate_integrity(dir.path(), &[]).unwrap();
        assert!(errors.is_empty());
    }

    #[test]
    fn sha256_of_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known");
        fs::write(&path, "").unwrap();

        let hash = sha256_file(&path).unwrap();
        // SHA-256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
