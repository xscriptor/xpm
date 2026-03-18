# Install and Upgrade Packages with xpm

This short guide explains the standard workflow to install new packages and update already installed packages.

## 1. Refresh Repository Databases

Always sync package metadata first:

```bash
xpm sync
```

## 2. Install Packages

Install one or more packages by name:

```bash
xpm install <package>
xpm install <package1> <package2>
```

Useful install variants:

```bash
xpm install --download-only <package>
xpm install --as-deps <package>
xpm install --as-explicit <package>
```

## 3. Upgrade Installed Packages

Upgrade all installed packages to the latest available versions:

```bash
xpm upgrade
```

Ignore specific packages during upgrade:

```bash
xpm upgrade --ignore <package>
```

## 4. Recommended Routine

For normal maintenance, run:

```bash
xpm sync
xpm upgrade
```

## Notes

- Use `--no-confirm` in automation scripts.
- Use `--config`, `--root`, `--dbpath`, and `--cachedir` to run in isolated environments.
- The `upgrade` command interface is in place and should be used after each successful `sync`.
- For non-root installations (custom `root_dir`), `xpm` creates command shims in `~/.local/bin` and ensures PATH export lines exist in `~/.bashrc` and `~/.zshrc`.
- If the current shell session still does not resolve a newly installed command, run `hash -r` or reload your shell config (`source ~/.zshrc` or `source ~/.bashrc`).
