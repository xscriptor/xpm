//! Repository database parsing for ALPM-style `.db` and `.files` archives.
//!
//! This module provides the foundation for sync database support (roadmap
//! phase 5), including parsing package metadata and file listings.

use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::XpmError;
use crate::XpmResult;

/// A package entry stored in a repository database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoEntry {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub arch: Option<String>,
    pub depends: Vec<String>,
    pub opt_depends: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
    pub replaces: Vec<String>,
    pub files: Vec<String>,
}

/// Parsed sync database for a repository (e.g. `core.db`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncDb {
    pub repo: String,
    pub entries: Vec<RepoEntry>,
}

/// In-memory representation of the local package database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalDb {
    pub entries: Vec<RepoEntry>,
}

#[derive(Debug, Default)]
struct ArchivePackage {
    desc: Option<String>,
    depends: Option<String>,
    files: Option<String>,
}

/// Parse a repository sync database archive (`<repo>.db`).
pub fn parse_sync_db(path: &Path, repo_name: &str) -> XpmResult<SyncDb> {
    let bytes = fs::read(path)?;
    parse_sync_db_bytes(&bytes, repo_name)
}

/// Parse a repository files database archive (`<repo>.files`) and merge file
/// listings into an existing [`SyncDb`].
pub fn merge_files_db(path: &Path, db: &mut SyncDb) -> XpmResult<()> {
    let bytes = fs::read(path)?;
    merge_files_db_bytes(&bytes, db)
}

fn parse_sync_db_bytes(bytes: &[u8], repo_name: &str) -> XpmResult<SyncDb> {
    let mut packages = read_repo_archive(bytes)?;
    let mut entries = Vec::new();

    for (pkg_id, pkg) in packages.drain() {
        let mut merged = HashMap::<String, Vec<String>>::new();

        if let Some(desc) = pkg.desc.as_deref() {
            merge_sections(&mut merged, parse_alpm_sections(desc));
        }
        if let Some(depends) = pkg.depends.as_deref() {
            merge_sections(&mut merged, parse_alpm_sections(depends));
        }

        let entry = repo_entry_from_sections(&pkg_id, &merged)?;
        entries.push(entry);
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.version.cmp(&b.version)));

    Ok(SyncDb {
        repo: repo_name.to_string(),
        entries,
    })
}

fn merge_files_db_bytes(bytes: &[u8], db: &mut SyncDb) -> XpmResult<()> {
    let mut packages = read_repo_archive(bytes)?;
    let mut files_by_id = HashMap::<String, Vec<String>>::new();

    for (pkg_id, pkg) in packages.drain() {
        let Some(files_content) = pkg.files else {
            continue;
        };

        let sections = parse_alpm_sections(&files_content);
        let files = sections
            .get("FILES")
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();

        files_by_id.insert(pkg_id, files);
    }

    for entry in &mut db.entries {
        let pkg_id = format!("{}-{}", entry.name, entry.version);
        if let Some(files) = files_by_id.remove(&pkg_id) {
            entry.files = files;
        }
    }

    Ok(())
}

fn read_repo_archive(bytes: &[u8]) -> XpmResult<HashMap<String, ArchivePackage>> {
    let mut archive = open_archive(bytes);
    let mut packages = HashMap::<String, ArchivePackage>::new();

    for entry_result in archive.entries()? {
        let mut entry = entry_result.map_err(|e| XpmError::Database(e.to_string()))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let path = entry
            .path()
            .map_err(|e| XpmError::Database(e.to_string()))?
            .into_owned();
        let path_str = path.to_string_lossy();
        let mut parts = path_str.split('/').filter(|p| !p.is_empty());
        let Some(pkg_id) = parts.next() else {
            continue;
        };
        let Some(file_name) = parts.next() else {
            continue;
        };

        let mut content = String::new();
        entry.read_to_string(&mut content)?;

        let pkg = packages.entry(pkg_id.to_string()).or_default();
        match file_name {
            "desc" => pkg.desc = Some(content),
            "depends" => pkg.depends = Some(content),
            "files" => pkg.files = Some(content),
            _ => {}
        }
    }

    Ok(packages)
}

fn open_archive(bytes: &[u8]) -> tar::Archive<Box<dyn Read>> {
    let reader: Box<dyn Read> = if is_gzip(bytes) {
        Box::new(GzDecoder::new(Cursor::new(bytes.to_vec())))
    } else {
        Box::new(Cursor::new(bytes.to_vec()))
    };

    tar::Archive::new(reader)
}

fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

fn merge_sections(into: &mut HashMap<String, Vec<String>>, from: HashMap<String, Vec<String>>) {
    for (k, mut v) in from {
        into.entry(k).or_default().append(&mut v);
    }
}

fn repo_entry_from_sections(
    pkg_id: &str,
    sections: &HashMap<String, Vec<String>>,
) -> XpmResult<RepoEntry> {
    let name = first_value(sections, "NAME").ok_or_else(|| {
        XpmError::Database(format!("missing %NAME% in repository entry '{pkg_id}'"))
    })?;
    let version = first_value(sections, "VERSION").ok_or_else(|| {
        XpmError::Database(format!("missing %VERSION% in repository entry '{pkg_id}'"))
    })?;

    Ok(RepoEntry {
        name,
        version,
        description: first_value(sections, "DESC"),
        arch: first_value(sections, "ARCH"),
        depends: values(sections, "DEPENDS"),
        opt_depends: values(sections, "OPTDEPENDS"),
        provides: values(sections, "PROVIDES"),
        conflicts: values(sections, "CONFLICTS"),
        replaces: values(sections, "REPLACES"),
        files: Vec::new(),
    })
}

fn first_value(sections: &HashMap<String, Vec<String>>, key: &str) -> Option<String> {
    sections.get(key).and_then(|values| values.first()).cloned()
}

fn values(sections: &HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
    sections.get(key).cloned().unwrap_or_default()
}

fn parse_alpm_sections(input: &str) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::<String, Vec<String>>::new();
    let mut current: Option<String> = None;

    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('%') && line.ends_with('%') && line.len() > 2 {
            let key = line.trim_matches('%').to_ascii_uppercase();
            current = Some(key);
            continue;
        }

        if let Some(section) = &current {
            map.entry(section.clone())
                .or_default()
                .push(line.to_string());
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gzip_tar(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gz);
            for (path, contents) in entries {
                let bytes = contents.as_bytes();
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, *path, bytes)
                    .expect("append tar entry");
            }
            builder.finish().expect("finish tar builder");
        }
        gz.finish().expect("finish gzip stream")
    }

    #[test]
    fn parse_sync_db_reads_desc_and_depends() {
        let bytes = make_gzip_tar(&[
            (
                "hello-1.0-1/desc",
                "%NAME%\nhello\n\n%VERSION%\n1.0-1\n\n%DESC%\nhello package\n\n%ARCH%\nx86_64\n",
            ),
            (
                "hello-1.0-1/depends",
                "%DEPENDS%\nlibc>=2.39\n\n%PROVIDES%\nhello-bin\n\n%CONFLICTS%\nhello-git\n",
            ),
            (
                "world-2.0-1/desc",
                "%NAME%\nworld\n\n%VERSION%\n2.0-1\n\n%DESC%\nworld package\n",
            ),
        ]);

        let db = parse_sync_db_bytes(&bytes, "core").expect("parse sync db");

        assert_eq!(db.repo, "core");
        assert_eq!(db.entries.len(), 2);

        let hello = db
            .entries
            .iter()
            .find(|e| e.name == "hello")
            .expect("hello entry exists");
        assert_eq!(hello.version, "1.0-1");
        assert_eq!(hello.description.as_deref(), Some("hello package"));
        assert_eq!(hello.arch.as_deref(), Some("x86_64"));
        assert_eq!(hello.depends, vec!["libc>=2.39"]);
        assert_eq!(hello.provides, vec!["hello-bin"]);
        assert_eq!(hello.conflicts, vec!["hello-git"]);
    }

    #[test]
    fn merge_files_db_adds_file_lists() {
        let db_bytes = make_gzip_tar(&[(
            "hello-1.0-1/desc",
            "%NAME%\nhello\n\n%VERSION%\n1.0-1\n\n%DESC%\nhello package\n",
        )]);
        let files_bytes = make_gzip_tar(&[(
            "hello-1.0-1/files",
            "%FILES%\nusr/\nusr/bin/\nusr/bin/hello\n",
        )]);

        let mut db = parse_sync_db_bytes(&db_bytes, "core").expect("parse sync db");
        merge_files_db_bytes(&files_bytes, &mut db).expect("merge files db");

        assert_eq!(db.entries.len(), 1);
        assert_eq!(
            db.entries[0].files,
            vec!["usr/", "usr/bin/", "usr/bin/hello"]
        );
    }

    #[test]
    fn parse_sync_db_reports_missing_required_fields() {
        let bytes = make_gzip_tar(&[("broken/desc", "%NAME%\nbroken\n")]);
        let err = parse_sync_db_bytes(&bytes, "core").expect_err("must fail");
        assert!(format!("{err}").contains("missing %VERSION%"));
    }

    #[test]
    fn parse_plain_tar_is_supported() {
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let contents = "%NAME%\nplain\n\n%VERSION%\n1.0-1\n";
            let bytes = contents.as_bytes();
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "plain-1.0-1/desc", bytes)
                .expect("append tar entry");
            builder.finish().expect("finish tar builder");
        }

        let db = parse_sync_db_bytes(&tar_bytes, "core").expect("parse plain tar");
        assert_eq!(db.entries.len(), 1);
        assert_eq!(db.entries[0].name, "plain");
    }
}
