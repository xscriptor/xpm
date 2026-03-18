//! Pre/post operation hooks for transactions.
//!
//! Hooks provide extensible points to execute arbitrary logic before and
//! after install, remove, and upgrade operations. Built-in hooks include
//! local database registration and file removal.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use tar;
use xz2::read::XzDecoder;
use zstd::Decoder;

use crate::error::{XpmError, XpmResult};

/// A hook that executes before or after an operation.
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self, context: &HookContext) -> XpmResult<()>;
}

/// Context passed to hooks during execution.
#[derive(Clone, Debug)]
pub struct HookContext {
    pub operation_type: OperationType,
    pub pkg_name: String,
    pub pkg_version: String,
    pub pkg_file: Option<PathBuf>,
    pub root_dir: PathBuf,
    pub local_db_dir: PathBuf,
    pub shell_integration: bool,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum OperationType {
    Install,
    Remove,
    Upgrade,
}

/// Extract package files to the filesystem.
pub struct FileExtractionHook;

impl Hook for FileExtractionHook {
    fn name(&self) -> &str {
        "file-extraction"
    }

    fn run(&self, context: &HookContext) -> XpmResult<()> {
        if context.operation_type == OperationType::Remove {
            return Ok(());
        }

        let pkg_file = context
            .pkg_file
            .as_ref()
            .ok_or_else(|| XpmError::Package("package file not specified".to_string()))?;

        if !pkg_file.exists() {
            return Err(XpmError::Package(format!(
                "package file not found: {}",
                pkg_file.display()
            )));
        }

        // Detect compression
        let file = fs::File::open(pkg_file)?;
        let mut magic = [0u8; 6];
        let n = std::io::BufReader::new(file).read(&mut magic)?;

        let reader: Box<dyn Read> = {
            let file = fs::File::open(pkg_file)?;
            let buf = std::io::BufReader::new(file);

            // Check magic bytes
            if n >= 4 && magic[..4] == [0x28, 0xB5, 0x2F, 0xFD] {
                // Zstd
                Box::new(Decoder::new(buf).map_err(|e| {
                    XpmError::Package(format!("failed to decode zstd: {}", e))
                })?)
            } else if n >= 2 && magic[..2] == [0x1F, 0x8B] {
                // Gzip
                Box::new(GzDecoder::new(buf))
            } else if n >= 6 && magic[..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
                // Xz
                Box::new(XzDecoder::new(buf))
            } else {
                // Uncompressed tar
                Box::new(buf)
            }
        };

        let mut archive = tar::Archive::new(reader);
        let mut installed_files: Vec<String> = Vec::new();
        let mut shell_shims: Vec<PathBuf> = Vec::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let path_str = path.to_string_lossy().replace('\\', "/");
            let normalized = path_str.trim_start_matches("./").to_string();

            // Skip metadata files and directories
            if normalized.is_empty()
                || normalized.ends_with(".PKGINFO")
                || normalized.ends_with(".BUILDINFO")
                || normalized.ends_with(".MTREE")
                || normalized.ends_with(".INSTALL")
                || normalized.ends_with('/')
            {
                continue;
            }

            let is_regular_file = entry.header().entry_type().is_file();
            let mode = entry.header().mode().unwrap_or(0);
            let is_executable = mode & 0o111 != 0;

            let target = context.root_dir.join(&normalized);

            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }

            entry.unpack(&target)?;
            installed_files.push(normalized.clone());

            // For non-root installations, create shims in ~/.local/bin for executable binaries.
            if context.shell_integration
                && is_regular_file
                && is_executable
                && normalized.starts_with("usr/bin/")
            {
                if let Some(shim) = ensure_shell_shim(&normalized, &target)? {
                    shell_shims.push(shim);
                }
            }
        }

        if context.shell_integration {
            ensure_shell_path_on_bash_zsh()?;
        }

        if !installed_files.is_empty() || !shell_shims.is_empty() {
            let pkg_dir = context.local_db_dir.join(&context.pkg_name);
            fs::create_dir_all(&pkg_dir)?;

            let files_path = pkg_dir.join("files");
            for shim in shell_shims {
                installed_files.push(format!("@ABS:{}", shim.display()));
            }

            fs::write(files_path, installed_files.join("\n"))?;
        }

        Ok(())
    }
}

fn ensure_shell_shim(relative_path: &str, target: &Path) -> XpmResult<Option<PathBuf>> {
    let home = match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => return Ok(None),
    };

    let local_bin = home.join(".local/bin");
    fs::create_dir_all(&local_bin)?;

    let Some(bin_name) = Path::new(relative_path).file_name() else {
        return Ok(None);
    };

    let shim_path = local_bin.join(bin_name);
    if shim_path.exists() || shim_path.symlink_metadata().is_ok() {
        let _ = fs::remove_file(&shim_path);
    }

    std::os::unix::fs::symlink(target, &shim_path)?;
    Ok(Some(shim_path))
}

fn ensure_shell_path_on_bash_zsh() -> XpmResult<()> {
    let home = match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h),
        None => return Ok(()),
    };

    let marker = "# xpm shell integration";
    let export_line = "export PATH=\"$HOME/.local/bin:$PATH\"";

    for rc in [".bashrc", ".zshrc"] {
        let rc_path = home.join(rc);
        if !rc_path.exists() {
            continue;
        }

        let content = fs::read_to_string(&rc_path).unwrap_or_default();
        if content.contains(marker) {
            continue;
        }

        let mut append = String::new();
        if !content.ends_with('\n') {
            append.push('\n');
        }
        append.push_str(marker);
        append.push('\n');
        append.push_str(export_line);
        append.push('\n');

        let mut merged = content;
        merged.push_str(&append);
        fs::write(rc_path, merged)?;
    }

    Ok(())
}

/// Register installed package in local database.
pub struct LocalDbHook;

impl Hook for LocalDbHook {
    fn name(&self) -> &str {
        "local-db"
    }

    fn run(&self, context: &HookContext) -> XpmResult<()> {
        match context.operation_type {
            OperationType::Install | OperationType::Upgrade => {
                let pkg_dir = context.local_db_dir.join(&context.pkg_name);
                fs::create_dir_all(&pkg_dir)?;

                // Write version file
                let version_path = pkg_dir.join("version");
                fs::write(version_path, &context.pkg_version)?;

                // Keep the file list generated by extraction if present.
                let files_path = pkg_dir.join("files");
                if !files_path.exists() {
                    fs::write(files_path, "")?;
                }

                Ok(())
            }
            OperationType::Remove => {
                let pkg_dir = context.local_db_dir.join(&context.pkg_name);
                if pkg_dir.exists() {
                    fs::remove_dir_all(pkg_dir)?;
                }
                Ok(())
            }
        }
    }
}

/// Remove package files from filesystem.
pub struct FileRemovalHook;

impl Hook for FileRemovalHook {
    fn name(&self) -> &str {
        "file-removal"
    }

    fn run(&self, context: &HookContext) -> XpmResult<()> {
        if context.operation_type != OperationType::Remove {
            return Ok(()); // Only applies to remove
        }

        let pkg_files_path = context.local_db_dir.join(&context.pkg_name).join("files");

        if !pkg_files_path.exists() {
            // Fallback cleanup for older installs without a manifest.
            fallback_remove_common_paths(context)?;
            return Ok(());
        }

        let file_list = fs::read_to_string(&pkg_files_path)?;
        if file_list.trim().is_empty() {
            // Fallback cleanup for older installs with empty manifest.
            fallback_remove_common_paths(context)?;
            return Ok(());
        }

        for file_path in file_list.lines() {
            if file_path.is_empty() {
                continue;
            }

            let target = if let Some(abs) = file_path.strip_prefix("@ABS:") {
                PathBuf::from(abs)
            } else {
                context.root_dir.join(file_path)
            };

            if target.is_dir() {
                let _ = fs::remove_dir(&target);
            } else if target.exists() || target.symlink_metadata().is_ok() {
                fs::remove_file(&target)?;
            }
        }

        Ok(())
    }
}

fn fallback_remove_common_paths(context: &HookContext) -> XpmResult<()> {
    // Conservative fallback: remove the common binary path matching package name.
    let bin_path = context.root_dir.join("usr/bin").join(&context.pkg_name);
    if bin_path.exists() || bin_path.symlink_metadata().is_ok() {
        let _ = fs::remove_file(&bin_path);
    }

    // Remove shims for both current user and original sudo user if available.
    for home in candidate_homes() {
        let shim = home.join(".local/bin").join(&context.pkg_name);
        if shim.exists() || shim.symlink_metadata().is_ok() {
            let _ = fs::remove_file(shim);
        }
    }

    Ok(())
}

fn candidate_homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();

    if let Some(h) = std::env::var_os("HOME") {
        homes.push(PathBuf::from(h));
    }

    if let Some(sudo_user) = std::env::var_os("SUDO_USER") {
        let sudo_user = sudo_user.to_string_lossy();
        let sudo_home = PathBuf::from("/home").join(sudo_user.as_ref());
        if sudo_home.exists() {
            homes.push(sudo_home);
        }
    }

    homes.sort();
    homes.dedup();
    homes
}

/// Load package metadata for inspection during hooks.
///
/// **Note:** Requires full package::reader integration.
pub struct MetadataLoadHook;

impl Hook for MetadataLoadHook {
    fn name(&self) -> &str {
        "metadata-load"
    }

    fn run(&self, context: &HookContext) -> XpmResult<()> {
        if let Some(pkg_file) = &context.pkg_file {
            if !pkg_file.exists() {
                return Err(XpmError::Package(format!(
                    "package file not found: {}",
                    pkg_file.display()
                )));
            }
            // TODO: Use package::reader::read_metadata() to load metadata
            // For now, just validate file exists
        }
        Ok(())
    }
}

/// Hook chain executor — runs multiple hooks in sequence.
pub struct HookChain {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookChain {
    pub fn new() -> Self {
        HookChain { hooks: Vec::new() }
    }

    pub fn add_hook(&mut self, hook: Box<dyn Hook>) {
        self.hooks.push(hook);
    }

    pub fn run(&self, context: &HookContext) -> XpmResult<()> {
        for hook in &self.hooks {
            let result = hook.run(context);
            if let Err(e) = result {
                return Err(XpmError::Package(format!(
                    "hook '{}' failed: {}",
                    hook.name(),
                    e
                )));
            }
        }
        Ok(())
    }

    pub fn hooks(&self) -> &[Box<dyn Hook>] {
        &self.hooks
    }
}

impl Default for HookChain {
    fn default() -> Self {
        let mut chain = HookChain::new();
        // Add default hooks in order
        chain.add_hook(Box::new(MetadataLoadHook));
        chain.add_hook(Box::new(FileExtractionHook));
        chain.add_hook(Box::new(FileRemovalHook));
        chain.add_hook(Box::new(LocalDbHook));
        chain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_context(op_type: OperationType) -> (TempDir, TempDir, HookContext) {
        let root_tmp = TempDir::new().expect("create root tempdir");
        let db_tmp = TempDir::new().expect("create db tempdir");

        let ctx = HookContext {
            operation_type: op_type,
            pkg_name: "test".to_string(),
            pkg_version: "1.0-1".to_string(),
            pkg_file: None,
            root_dir: root_tmp.path().to_path_buf(),
            local_db_dir: db_tmp.path().to_path_buf(),
            shell_integration: false,
        };

        (root_tmp, db_tmp, ctx)
    }

    #[test]
    fn hook_chain_default_has_hooks() {
        let chain = HookChain::default();
        assert!(!chain.hooks().is_empty());
        assert_eq!(chain.hooks().len(), 4); // metadata, extraction, removal, localdb
    }

    #[test]
    fn local_db_hook_creates_version_file() {
        let (_root, _db_tmp, ctx) = test_context(OperationType::Install);
        let hook = LocalDbHook;

        hook.run(&ctx).expect("run hook");

        let version_file = ctx.local_db_dir.join(&ctx.pkg_name).join("version");
        assert!(version_file.exists());

        let content = fs::read_to_string(&version_file).expect("read version file");
        assert_eq!(content, "1.0-1");
    }

    #[test]
    fn local_db_hook_remove_deletes_entry() {
        let (_root, _db_tmp, ctx) = test_context(OperationType::Remove);
        let pkg_dir = ctx.local_db_dir.join(&ctx.pkg_name);

        // Create the entry first
        fs::create_dir_all(&pkg_dir).expect("create pkg dir");
        fs::write(pkg_dir.join("version"), "1.0-1").expect("write version");

        // Now remove it
        let hook = LocalDbHook;
        hook.run(&ctx).expect("run hook");

        assert!(!pkg_dir.exists(), "pkg directory should be deleted");
    }

    #[test]
    fn file_removal_hook_skips_if_no_files() {
        let (_root, _db_tmp, ctx) = test_context(OperationType::Remove);
        let hook = FileRemovalHook;

        // Should not error even if no file list exists
        let result = hook.run(&ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn hook_chain_runs_all_hooks() {
        let (_root, _db_tmp, ctx) = test_context(OperationType::Install);
        let mut chain = HookChain::new();
        chain.add_hook(Box::new(LocalDbHook));

        let result = chain.run(&ctx);
        assert!(result.is_ok());
    }
}
