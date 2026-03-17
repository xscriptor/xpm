//! Pre/post operation hooks for transactions.
//!
//! Hooks provide extensible points to execute arbitrary logic before and
//! after install, remove, and upgrade operations. Built-in hooks include
//! local database registration and file removal.

use std::fs;
use std::path::PathBuf;

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
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum OperationType {
    Install,
    Remove,
    Upgrade,
}

/// Extract package files to the filesystem.
///
/// **Note:** Full implementation requires package archive reader integration.
/// Currently this is a placeholder for post-preparation.
pub struct FileExtractionHook;

impl Hook for FileExtractionHook {
    fn name(&self) -> &str {
        "file-extraction"
    }

    fn run(&self, context: &HookContext) -> XpmResult<()> {
        if context.operation_type == OperationType::Remove {
            return Ok(()); // No files to extract for remove
        }

        let _pkg_file = context
            .pkg_file
            .as_ref()
            .ok_or_else(|| XpmError::Package("package file not specified".to_string()))?;

        // TODO: Integrate with package::reader to extract files
        //       This requires reading .xp archives and extracting to root_dir

        Ok(())
    }
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

                // TODO: Write file list from .MTREE
                let files_path = pkg_dir.join("files");
                fs::write(files_path, "")?;

                Ok(())
            }
            OperationType::Remove => {
                let pkg_dir = context.local_db_dir.join(&context.pkg_name);
                fs::remove_dir_all(pkg_dir)?;
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
            // No file list; skip removal
            return Ok(());
        }

        let file_list = fs::read_to_string(&pkg_files_path)?;

        for file_path in file_list.lines() {
            if file_path.is_empty() {
                continue;
            }

            let target = context.root_dir.join(file_path);

            if target.is_dir() {
                let _ = fs::remove_dir(&target);
            } else if target.exists() {
                fs::remove_file(&target)?;
            }
        }

        Ok(())
    }
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
        };

        (root_tmp, db_tmp, ctx)
    }

    #[test]
    fn hook_chain_default_has_hooks() {
        let chain = HookChain::default();
        assert!(!chain.hooks().is_empty());
        assert_eq!(chain.hooks().len(), 3); // metadata, extraction, localdb
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
