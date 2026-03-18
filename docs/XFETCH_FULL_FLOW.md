# Full Install Flow Example: xfetch

This document records a full end-to-end installation flow using `xpm` and `xfetch`.

## Goal

Validate that `xpm` can:

1. Sync repository metadata.
2. Resolve and download `xfetch`.
3. Commit the transaction and extract files.
4. Register the package in the local package database.
5. Run the installed application.

## Test Environment

The flow was executed in an isolated environment (not system root):

- `root_dir = /tmp/xpm-root`
- `db_path = /tmp/xpm-db`
- `cache_dir = /tmp/xpm-cache`
- custom config file: `/tmp/xpm-local.conf`
- repository mirror source: local mirror derived from x-repo

## Commands Used

### 1) Sync

```bash
xpm --config /tmp/xpm-local.conf sync
```

Observed result:

- repository `x` synced successfully
- local parsed sync DB reported `1 package(s) loaded`

### 2) Install xfetch

```bash
xpm --config /tmp/xpm-local.conf --no-confirm install xfetch
```

Observed result:

- package downloaded to `/tmp/xpm-cache/xfetch-0.1.0-1-x86_64.xp`
- transaction prepared and committed
- output confirmed `1 package(s) installed successfully`

## Post-Install Verification

### A) Binary extracted

Verified file exists and is executable:

- `/tmp/xpm-root/usr/bin/xfetch`

### B) Local DB entry created

Verified version file:

- `/tmp/xpm-db/local/xfetch/version`
- content: `0.1.0-1`

### C) App runs

Executed:

```bash
/tmp/xpm-root/usr/bin/xfetch
```

Result:

- application executed successfully and printed system information output

### D) Command-line shim available (bash/zsh)

For the isolated non-root install, `xpm` created a shim:

- `~/.local/bin/xfetch -> /tmp/xpm-root/usr/bin/xfetch`

Interactive shell checks confirmed command discovery:

- `zsh -ic 'command -v xfetch'` returned `~/.local/bin/xfetch`
- `bash -ic 'command -v xfetch'` returned `~/.local/bin/xfetch`

## Conclusion

The install flow is operational end-to-end for this scenario: sync -> download -> prepare -> commit -> extraction -> local DB registration -> runnable binary.
