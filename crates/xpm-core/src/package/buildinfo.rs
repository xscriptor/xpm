//! Parser for `.BUILDINFO` files.
//!
//! Same `key = value` format as `.PKGINFO`, recording the build environment
//! for reproducibility purposes.

use crate::error::XpmError;
use crate::package::types::BuildInfo;

// ── Public API ────────────────────────────────────────────────

/// Parse the raw bytes of a `.BUILDINFO` file into a [`BuildInfo`].
pub fn parse_buildinfo(data: &[u8]) -> Result<BuildInfo, XpmError> {
    let text = std::str::from_utf8(data)
        .map_err(|e| XpmError::Package(format!("invalid UTF-8 in .BUILDINFO: {e}")))?;

    let mut info = BuildInfo::default();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = match line.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };

        match key {
            "pkgname" => info.pkgname = value.to_string(),
            "pkgver" => info.pkgver = value.to_string(),
            "builddate" => info.builddate = value.parse().unwrap_or(0),
            "builddir" => info.builddir = value.to_string(),
            "buildtool" => info.buildtool = value.to_string(),
            "buildhost" => info.buildhost = value.to_string(),
            "compress" => info.compress = value.to_string(),
            "compress_level" => info.compress_level = value.parse().unwrap_or(0),
            "CFLAGS" => info.cflags = value.to_string(),
            "CXXFLAGS" => info.cxxflags = value.to_string(),
            "LDFLAGS" => info.ldflags = value.to_string(),
            "MAKEFLAGS" => info.makeflags = value.to_string(),
            _ => {} // Ignore unknown fields for forward-compatibility.
        }
    }

    Ok(info)
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_complete_buildinfo() {
        let data = "\
            pkgname = hello\n\
            pkgver = 1.0.0-1\n\
            builddate = 1700000000\n\
            builddir = /tmp/xpkg/hello\n\
            buildtool = xpkg 0.1.0\n\
            buildhost = builder.local\n\
            compress = zstd\n\
            compress_level = 3\n\
            CFLAGS = -O2 -pipe\n\
            CXXFLAGS = -O2 -pipe\n\
            LDFLAGS = -Wl,-O1\n\
            MAKEFLAGS = -j8\n";

        let info = parse_buildinfo(data.as_bytes()).unwrap();
        assert_eq!(info.pkgname, "hello");
        assert_eq!(info.pkgver, "1.0.0-1");
        assert_eq!(info.builddate, 1700000000);
        assert_eq!(info.builddir, "/tmp/xpkg/hello");
        assert_eq!(info.buildtool, "xpkg 0.1.0");
        assert_eq!(info.buildhost, "builder.local");
        assert_eq!(info.compress, "zstd");
        assert_eq!(info.compress_level, 3);
        assert_eq!(info.cflags, "-O2 -pipe");
        assert_eq!(info.makeflags, "-j8");
    }

    #[test]
    fn parse_minimal_buildinfo() {
        let data = "pkgname = test\npkgver = 1-1\n";
        let info = parse_buildinfo(data.as_bytes()).unwrap();
        assert_eq!(info.pkgname, "test");
        assert_eq!(info.compress, "");
        assert_eq!(info.compress_level, 0);
    }

    #[test]
    fn parse_skips_unknown_keys() {
        let data = "pkgname = test\nfuture_field = value\n";
        let info = parse_buildinfo(data.as_bytes()).unwrap();
        assert_eq!(info.pkgname, "test");
    }

    #[test]
    fn parse_empty_buildinfo() {
        let info = parse_buildinfo(b"").unwrap();
        assert!(info.pkgname.is_empty());
    }
}
