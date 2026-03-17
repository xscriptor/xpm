//! Remote repository database synchronization.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::Repository;
use crate::{XpmError, XpmResult};

/// Result metadata for a repository sync operation.
#[derive(Debug, Clone)]
pub struct RepoSyncResult {
    pub mirror: String,
    pub db_downloaded: bool,
    pub files_downloaded: bool,
}

/// Expand `$repo` and `$arch` placeholders in a repository server URL.
pub fn expand_repo_url(server: &str, repo: &str, arch: &str) -> String {
    server.replace("$repo", repo).replace("$arch", arch)
}

/// Download `<repo>.db` and optional `<repo>.files` into `sync_dir`.
///
/// The operation tries each configured mirror until one succeeds for `.db`.
/// The `.files` download is best-effort and does not fail the whole sync.
pub fn sync_repo_databases(
    repo: &Repository,
    arch: &str,
    sync_dir: &Path,
    retries: u32,
) -> XpmResult<RepoSyncResult> {
    fs::create_dir_all(sync_dir)?;

    let db_dest = sync_dir.join(format!("{}.db", repo.name));
    let files_dest = sync_dir.join(format!("{}.files", repo.name));

    let mut last_error: Option<XpmError> = None;

    for server in &repo.server {
        let base = expand_repo_url(server, &repo.name, arch);
        let base = base.trim_end_matches('/');
        let db_url = format!("{base}/{}.db", repo.name);

        match download_with_retries(&db_url, &db_dest, retries) {
            Ok(()) => {
                let files_url = format!("{base}/{}.files", repo.name);
                let files_downloaded = download_with_retries(&files_url, &files_dest, retries)
                    .map(|_| true)
                    .unwrap_or(false);

                return Ok(RepoSyncResult {
                    mirror: base.to_string(),
                    db_downloaded: true,
                    files_downloaded,
                });
            }
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        XpmError::Database(format!(
            "no usable mirror found for repository '{}'",
            repo.name
        ))
    }))
}

fn download_with_retries(url: &str, dest: &Path, retries: u32) -> XpmResult<()> {
    let attempts = retries.max(1);
    let mut last_error: Option<XpmError> = None;

    for attempt in 1..=attempts {
        match download_once(url, dest) {
            Ok(()) => return Ok(()),
            Err(err) => {
                tracing::warn!(url, attempt, attempts, error = %err, "download attempt failed");
                last_error = Some(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| XpmError::Database(format!("download failed: {url}"))))
}

fn download_once(url: &str, dest: &Path) -> XpmResult<()> {
    if let Some(path) = local_file_path(url) {
        tracing::debug!(url, source = %path.display(), "syncing database from local file mirror");
        return copy_local_file(&path, dest);
    }

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| XpmError::Database(format!("http client init failed: {e}")))?;

    let mut response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| XpmError::Database(format!("{url}: {e}")))?;

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let total = response.content_length();
    let temp_path = dest.with_extension("part");
    let mut file = fs::File::create(&temp_path)?;
    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 65536];
    let mut next_report: u64 = 25;

    loop {
        let n = response
            .read(&mut buffer)
            .map_err(|e| XpmError::Database(format!("{url}: {e}")))?;
        if n == 0 {
            break;
        }

        file.write_all(&buffer[..n])?;
        downloaded += n as u64;

        if let Some(total) = total {
            if total > 0 {
                let percent = downloaded.saturating_mul(100) / total;
                if percent >= next_report {
                    tracing::debug!(url, downloaded, total, percent, "download progress");
                    next_report = (next_report + 25).min(100);
                }
            }
        }
    }

    file.flush()?;
    fs::rename(&temp_path, dest)?;

    tracing::debug!(url, downloaded, dest = %dest.display(), "download complete");
    Ok(())
}

fn local_file_path(url: &str) -> Option<PathBuf> {
    let raw = url.strip_prefix("file://")?;
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

fn copy_local_file(source: &Path, dest: &Path) -> XpmResult<()> {
    let data = fs::read(source)?;
    write_atomically(dest, &data)
}

fn write_atomically(dest: &Path, bytes: &[u8]) -> XpmResult<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = dest.with_extension("part");
    let mut file = fs::File::create(&temp_path)?;
    file.write_all(bytes)?;
    file.flush()?;
    fs::rename(&temp_path, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_repo_url_substitutes_placeholders() {
        let url = expand_repo_url(
            "https://mirror.example.com/$repo/os/$arch",
            "core",
            "x86_64",
        );
        assert_eq!(url, "https://mirror.example.com/core/os/x86_64");
    }

    #[test]
    fn local_file_repo_sync_downloads_db_and_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mirror = temp.path().join("mirror");
        let sync = temp.path().join("sync");
        fs::create_dir_all(&mirror).expect("create mirror dir");

        fs::write(mirror.join("core.db"), b"db-data").expect("write db");
        fs::write(mirror.join("core.files"), b"files-data").expect("write files");

        let repo = Repository {
            name: "core".to_string(),
            server: vec![format!("file://{}", mirror.display())],
            sig_level: None,
        };

        let result = sync_repo_databases(&repo, "x86_64", &sync, 2).expect("sync repo");
        assert!(result.db_downloaded);
        assert!(result.files_downloaded);
        assert!(sync.join("core.db").exists());
        assert!(sync.join("core.files").exists());
    }
}
