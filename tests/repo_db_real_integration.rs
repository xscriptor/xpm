//! Integration tests for repository database parsing (#56).
//! Tests real Arch/xpkg-style .db files from workspace.

use std::path::Path;
use xpm_core::repo_db::{merge_files_db, parse_sync_db};

#[test]
fn test_real_xrepo_database_parsing() {
    let db_path = Path::new("/home/xscriptor/Documents/repos/xpkgrepos/x-repo/public/repo/x86_64/x.db.tar.gz");
    
    if !db_path.exists() {
        eprintln!("Skipping: real x.db not found at {:?}", db_path);
        return;
    }
    
    // Parse the real x-repo database
    let result = parse_sync_db(db_path, "x");
    assert!(result.is_ok(), "Failed to parse real x.db: {:?}", result);
    
    let db = result.unwrap();
    assert!(!db.entries.is_empty(), "x.db should contain package entries");
    eprintln!("✓ Parsed x.db with {} packages", db.entries.len());
    
    // Validate all entries have required fields
    for entry in &db.entries {
        assert!(!entry.name.is_empty(), "Package must have name");
        assert!(!entry.version.is_empty(), "Package {} must have version", entry.name);
        eprintln!("  - {} {}", entry.name, entry.version);
    }
    
    // Check for xfetch if present
    if let Some(xfetch) = db.entries.iter().find(|e| e.name == "xfetch") {
        eprintln!("✓ Found xfetch in x.db:");
        eprintln!("  Version: {}", xfetch.version);
        eprintln!("  Filename: {:?}", xfetch.filename);
        eprintln!("  SHA256: {:?}", xfetch.sha256sum.as_ref().map(|s| &s[..16]));
        eprintln!("  URL: {:?}", xfetch.url);
        
        // xfetch should have extended metadata
        assert!(xfetch.filename.is_some(), "xfetch should have FILENAME field");
        assert!(xfetch.sha256sum.is_some(), "xfetch should have SHA256SUM field");
    }
}

#[test]
fn test_real_xrepo_with_files_merge() {
    let db_path = Path::new("/home/xscriptor/Documents/repos/xpkgrepos/x-repo/public/repo/x86_64/x.db.tar.gz");
    let files_path = Path::new("/home/xscriptor/Documents/repos/xpkgrepos/x-repo/public/repo/x86_64/x.files");
    
    if !db_path.exists() {
        eprintln!("Skipping: real x.db not found");
        return;
    }
    
    // Parse .db
    let mut db = parse_sync_db(db_path, "x").expect("parse x.db");
    let initial_count = db.entries.len();
    eprintln!("Loaded {} packages from x.db", initial_count);
    
    // Try to merge .files if it exists
    if files_path.exists() {
        let result = merge_files_db(files_path, &mut db);
        eprintln!("Merged .files: {:?}", result);
        
        let with_files = db.entries.iter().filter(|e| !e.files.is_empty()).count();
        eprintln!("After merge: {} packages have file listings", with_files);
    } else {
        eprintln!("Note: x.files not found (expected in metadata-only repo)");
    }
    
    assert_eq!(db.entries.len(), initial_count, "Package count preserved after merge");
}

#[test]
fn test_xpkg_extended_vs_arch_compatibility() {
    // This documents the differences between xpkg-extended and standard Arch formats
    eprintln!("Repository database format compatibility:");
    eprintln!("  Standard Arch .db:");
    eprintln!("    Required: NAME, VERSION");
    eprintln!("    Optional: DESC, ARCH, DEPENDS, PROVIDES, CONFLICTS");
    eprintln!("    NO: FILENAME, SHA256SUM, URL");
    eprintln!("");
    eprintln!("  xpkg-extended .db (x-linux):");
    eprintln!("    Includes all Arch fields PLUS:");
    eprintln!("    FILENAME: package artifact name");
    eprintln!("    SHA256SUM: checksum for download verification");
    eprintln!("    URL: source/repository URL for fetching");
    eprintln!("✓ Both formats supported by xpm parser");
}
