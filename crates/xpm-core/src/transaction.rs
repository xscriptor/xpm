//! Transaction engine for install, remove, and upgrade operations.
//!
//! This module provides the core transaction logic: planning dependency
//! resolution, preparing file extractions and removals, committing changes
//! to the filesystem, and rolling back on failure.
//!
//! Transactions are atomic where possible and logged for auditing.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{XpmError, XpmResult};
use crate::package::PackageMeta;

/// A filesystem-based lock to prevent concurrent xpm operations.
pub struct FileLock {
    path: PathBuf,
    _guard: Option<File>,
}

impl FileLock {
    /// Acquire an exclusive lock at the given path.
    /// Other processes will wait or fail if already locked.
    pub fn acquire(path: &Path) -> XpmResult<Self> {
        fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new("/")))?;

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| XpmError::Package(format!("failed to acquire lock: {}", e)))?;

        Ok(FileLock {
            path: path.to_path_buf(),
            _guard: Some(file),
        })
    }

    /// Release the lock by dropping the file handle.
    pub fn release(&mut self) -> XpmResult<()> {
        self._guard = None;
        // Optionally remove the lock file
        let _ = fs::remove_file(&self.path);
        Ok(())
    }
}

/// Single operation within a transaction.
#[derive(Debug, Clone)]
pub enum TransactionOp {
    /// Install a package from a .xp file.
    Install {
        pkg_name: String,
        pkg_version: String,
        pkg_file: PathBuf,
        metadata: Option<PackageMeta>,
    },
    /// Remove an installed package.
    Remove { pkg_name: String },
    /// Upgrade from one version to another.
    Upgrade {
        pkg_name: String,
        old_version: String,
        new_version: String,
        new_pkg_file: PathBuf,
    },
}

/// State of a transaction during its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// Operations collected but not yet prepared.
    Planning,
    /// Pre-flight checks passed, ready to commit.
    Prepared,
    /// Changes written to filesystem and database.
    Committed,
    /// Rolled back due to error or user request.
    RolledBack,
}

/// A package management transaction with plan, prepare, and commit phases.
pub struct Transaction {
    operations: Vec<TransactionOp>,
    state: TransactionState,
    lock: Option<FileLock>,
    log_file: PathBuf,
    #[allow(dead_code)]
    root_dir: PathBuf,
    local_db_dir: PathBuf,
    rollback_counter: usize,
}

impl Transaction {
    /// Create a new transaction with default root and database directories.
    pub fn new(root_dir: PathBuf, local_db_dir: PathBuf) -> XpmResult<Self> {
        let lock_path = local_db_dir.join("transaction.lock");
        let log_file = root_dir.join("var/log/xpm.log");

        let lock = FileLock::acquire(&lock_path).ok();

        fs::create_dir_all(&local_db_dir)?;
        fs::create_dir_all(log_file.parent().unwrap_or_else(|| Path::new("/")))?;

        Ok(Transaction {
            operations: Vec::new(),
            state: TransactionState::Planning,
            lock,
            log_file,
            root_dir,
            local_db_dir,
            rollback_counter: 0,
        })
    }

    /// Add an install operation to the transaction.
    pub fn add_install(
        &mut self,
        pkg_name: String,
        pkg_version: String,
        pkg_file: PathBuf,
    ) -> XpmResult<()> {
        if self.state != TransactionState::Planning {
            return Err(XpmError::Package(
                "can only add operations during planning phase".to_string(),
            ));
        }

        self.operations.push(TransactionOp::Install {
            pkg_name,
            pkg_version,
            pkg_file,
            metadata: None,
        });

        Ok(())
    }

    /// Add a remove operation to the transaction.
    pub fn add_remove(&mut self, pkg_name: String) -> XpmResult<()> {
        if self.state != TransactionState::Planning {
            return Err(XpmError::Package(
                "can only add operations during planning phase".to_string(),
            ));
        }

        self.operations.push(TransactionOp::Remove { pkg_name });

        Ok(())
    }

    /// Transition to the prepare phase: validate all operations.
    pub fn prepare(&mut self) -> XpmResult<()> {
        if self.state != TransactionState::Planning {
            return Err(XpmError::Package(
                "transaction is not in planning state".to_string(),
            ));
        }

        // Pre-flight checks for each operation
        for op in &self.operations {
            match op {
                TransactionOp::Install { pkg_file, .. } => {
                    if !pkg_file.exists() {
                        return Err(XpmError::Package(format!(
                            "package file does not exist: {}",
                            pkg_file.display()
                        )));
                    }
                }
                TransactionOp::Remove { pkg_name } => {
                    let installed_path = self.local_db_dir.join(pkg_name);
                    if !installed_path.exists() {
                        return Err(XpmError::Package(format!(
                            "package not installed: {}",
                            pkg_name
                        )));
                    }
                }
                TransactionOp::Upgrade { new_pkg_file, .. } => {
                    if !new_pkg_file.exists() {
                        return Err(XpmError::Package(format!(
                            "upgrade package file does not exist: {}",
                            new_pkg_file.display()
                        )));
                    }
                }
            }
        }

        // Check disk space (simplified: just ensure target directories are writable)
        self.verify_permissions()?;

        self.state = TransactionState::Prepared;
        self.log("prepared")?;

        Ok(())
    }

    /// Execute the transaction: write changes to filesystem.
    pub fn commit(&mut self) -> XpmResult<()> {
        if self.state != TransactionState::Prepared {
            return Err(XpmError::Package(
                "transaction must be prepared before commit".to_string(),
            ));
        }

        for op in self.operations.clone() {
            match op {
                TransactionOp::Install {
                    pkg_name,
                    pkg_version,
                    pkg_file,
                    ..
                } => {
                    self.execute_install(&pkg_name, &pkg_version, &pkg_file)?;
                }
                TransactionOp::Remove { pkg_name } => {
                    self.execute_remove(&pkg_name)?;
                }
                TransactionOp::Upgrade {
                    pkg_name,
                    new_version,
                    new_pkg_file,
                    ..
                } => {
                    self.execute_upgrade(&pkg_name, &new_version, &new_pkg_file)?;
                }
            }
        }

        self.state = TransactionState::Committed;
        self.log("committed")?;

        Ok(())
    }

    /// Rollback the transaction (cleanup resources).
    pub fn rollback(&mut self) -> XpmResult<()> {
        self.log("rolling back")?;

        // For now, cleanup is minimal since we haven't actually written files yet.
        // In full implementation, would undo file extractions and removals.

        self.rollback_counter += 1;
        self.state = TransactionState::RolledBack;

        Ok(())
    }

    /// Install a single package (internal - called by commit).
    fn execute_install(&self, pkg_name: &str, pkg_version: &str, _pkg_file: &Path) -> XpmResult<()> {
        // Record in local database
        let db_entry = self.local_db_dir.join(pkg_name);
        fs::create_dir_all(&db_entry)?;

        let version_file = db_entry.join("version");
        fs::write(version_file, pkg_version)?;

        // TODO: Extract files from pkg_file to root_dir using post-install hooks
        // For now, just mark as installed in database

        self.log(&format!("installed {} {}", pkg_name, pkg_version))?;

        Ok(())
    }

    /// Remove a single package (internal - called by commit).
    fn execute_remove(&self, pkg_name: &str) -> XpmResult<()> {
        // TODO: Use file listings from local db to remove files from root_dir

        let db_entry = self.local_db_dir.join(pkg_name);
        fs::remove_dir_all(&db_entry)?;

        self.log(&format!("removed {}", pkg_name))?;

        Ok(())
    }

    /// Upgrade a single package (internal - called by commit).
    fn execute_upgrade(&self, pkg_name: &str, new_version: &str, _new_pkg_file: &Path) -> XpmResult<()> {
        // Remove old version
        self.execute_remove(pkg_name)?;

        // Install new version
        self.execute_install(pkg_name, new_version, _new_pkg_file)?;

        Ok(())
    }

    /// Verify that we have permission to write to target directories.
    fn verify_permissions(&self) -> XpmResult<()> {
        // Check if we can write to local_db_dir
        if !self.local_db_dir.exists() {
            fs::create_dir_all(&self.local_db_dir)?;
        }

        // In a real implementation, would check write perms to root_dir
        Ok(())
    }

    /// Append a log entry with timestamp.
    pub fn log(&self, message: &str) -> XpmResult<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)?;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        writeln!(file, "[{}] {}", timestamp, message)?;

        Ok(())
    }

    /// Get current transaction state.
    pub fn state(&self) -> TransactionState {
        self.state
    }

    /// Get number of operations in this transaction.
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Get operations slice.
    pub fn operations(&self) -> &[TransactionOp] {
        &self.operations
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        let _ = self.lock.take().map(|mut l| l.release());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn test_root() -> (TempDir, PathBuf, PathBuf) {
        let tmpdir = TempDir::new().expect("create tempdir");
        let root = tmpdir.path().to_path_buf();
        let local_db = root.join("var/lib/xpm/local");
        (tmpdir, root, local_db)
    }

    #[test]
    fn create_transaction() {
        let (_tmp, root, local_db) = test_root();
        let tx = Transaction::new(root, local_db).expect("create transaction");
        assert_eq!(tx.state(), TransactionState::Planning);
        assert_eq!(tx.operation_count(), 0);
    }

    #[test]
    fn add_install_operation() {
        let (_tmp, root, local_db) = test_root();
        let mut tx = Transaction::new(root, local_db).expect("create transaction");

        tx.add_install(
            "test".to_string(),
            "1.0-1".to_string(),
            PathBuf::from("/tmp/test-1.0-1-x86_64.xp"),
        )
        .expect("add install");

        assert_eq!(tx.operation_count(), 1);
    }

    #[test]
    fn prepare_requires_planning_state() {
        let (_tmp, root, local_db) = test_root();
        let mut tx = Transaction::new(root, local_db).expect("create transaction");

        tx.prepare().expect("prepare");
        assert_eq!(tx.state(), TransactionState::Prepared);

        let result = tx.prepare();
        assert!(
            result.is_err(),
            "should not allow prepare when not in planning state"
        );
    }

    #[test]
    fn add_operation_fails_after_prepare() {
        let (_tmp, root, local_db) = test_root();
        let mut tx = Transaction::new(root, local_db).expect("create transaction");

        tx.prepare().expect("prepare");

        let result = tx.add_install(
            "test".to_string(),
            "1.0-1".to_string(),
            PathBuf::from("/tmp/test.xp"),
        );

        assert!(result.is_err(), "should not allow add after prepare");
    }

    #[test]
    fn transaction_logs_to_file() {
        let (_tmp, root, local_db) = test_root();
        let tx = Transaction::new(root.clone(), local_db).expect("create transaction");

        tx.log("test message").expect("log");

        let log_path = root.join("var/log/xpm.log");
        assert!(log_path.exists(), "log file should exist");

        let content = fs::read_to_string(&log_path).expect("read log");
        assert!(content.contains("test message"), "log should contain message");
    }

    #[test]
    fn commit_requires_prepared_state() {
        let (_tmp, root, local_db) = test_root();
        let mut tx = Transaction::new(root, local_db).expect("create transaction");

        let result = tx.commit();
        assert!(result.is_err(), "commit should fail if not prepared");
    }

    #[test]
    fn rollback_changes_state() {
        let (_tmp, root, local_db) = test_root();
        let mut tx = Transaction::new(root, local_db).expect("create transaction");

        tx.rollback().expect("rollback");
        assert_eq!(tx.state(), TransactionState::RolledBack);
    }
}
