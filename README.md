# unused-removal

Fast, cross-platform file scanner and cleaner. Single static binary (~15 MB), no dependencies. Scans disks in seconds, finds large/junk/old files and duplicates, provides a polished web UI, and deletes safely to the Recycle Bin (reversible).

## Features

| Feature | Description |
|---------|-------------|
| **Parallel scanning** | Multi-threaded walker using all CPU cores. 100k+ files/sec. |
| **Smart categorization** | Large/huge files, junk (temp/cache/logs), stale files, duplicates, old installers, app leftovers. |
| **Incremental cache** | Embedded database (redb) — repeat scans are near-instant (only changed directories re-scanned). |
| **Web interface** | Built-in HTTP server on `127.0.0.1`. Modern SPA with table, filters, sorting, multi-select, export. |
| **Safe deletion** | Default: Recycle Bin (reversible). Hard delete requires explicit confirmation. |
| **System protection** | Windows: WinSxS, System32, Program Files, pagefile.sys, hiberfil.sys — never suggested for deletion. macOS/Linux: `/System`, `/Library`, `/bin`, `/sbin`, etc. |
| **Zero dependencies** | Single statically-linked binary. Works on Windows 10/11, macOS 12+, Linux. |

## Installation

Download `unused-removal` from [Releases](https://github.com/inajaf/unused-removal/releases) or build from source:

```bash
# Requires Rust 1.75+
git clone https://github.com/inajaf/unused-removal
cd unused-removal
cargo build --release
# Binary at ./target/release/unused-removal (or unused-removal.exe on Windows)
```

## Usage

### Web UI (recommended)

```bash
unused-removal serve --port 8080
# Opens http://127.0.0.1:8080 in your browser
```

1. Select a drive or folder (C:\, D:\, /home, custom path)
2. Adjust options (workers, cache, duplicates, system protection)
3. Click **Start Scan**
4. Filter, sort, and select files in the results table
5. **Move to Trash** — safe, recoverable
6. **Delete Permanently** — only after explicit confirmation

### CLI — Scan

```bash
# Quick scan with JSON report
unused-removal scan --root /home/user --json report.json

# Only files > 500 MB, no cache
unused-removal scan --root /data --large 500MB --no-cache

# Include duplicate detection (slower)
unused-removal scan --root /home --duplicates
```

### CLI — Benchmark

```bash
# Compare parallel vs serial scanning
unused-removal bench --files 100000 --depth 4 --serial
```

### Configuration

```bash
# Show current config
unused-removal config
```

Config file locations (in order): `./config.toml` → `$XDG_CONFIG_HOME/unused-removal/config.toml` (Linux/macOS) / `%LOCALAPPDATA%\unused-removal\config.toml` (Windows) → built-in defaults.

## Configuration (`config.toml`)

```toml
root = "/"                    # Scan root (Windows: "C:\\")
workers = 0                   # 0 = all CPU cores
follow_links = false          # Follow symlinks/junctions
use_cache = true              # Incremental cache (redb)
check_duplicates = false      # Duplicate detection (slower)
protect_system = true         # Skip system/critical paths

# Size thresholds
large_bytes = 104857600       # 100 MiB
huge_bytes = 524288000        # 500 MiB

# Time thresholds (days)
stale_days = 180              # Not modified for N days
old_log_days = 30             # Log files older than N days
stale_install_days = 90       # Installers in Downloads

# Junk patterns
junk_extensions = [".tmp", ".temp", ".bak", ".old", ".dmp", ".chk", "~$*"]
junk_dirs = [
  "/tmp",
  "/var/tmp",
  "~/Library/Caches",
  "~/.cache",
  "%TEMP%",
  "C:\\Windows\\Temp",
  "C:\\Windows\\Prefetch"
]

# Excluded from traversal
exclude_dirs = [
  "$Recycle.Bin",
  "System Volume Information",
  "Windows\\WinSxS",
  "/proc",
  "/sys",
  "/dev"
]
```

## Architecture

```
unused-removal/
├── src/
│   ├── main.rs               # CLI entry: scan / serve / bench / config
│   ├── scanner/              # Parallel filesystem walker
│   │   ├── mod.rs            # Worker pool, task queue, progress
│   │   ├── platform/         # OS-specific implementations
│   │   │   ├── windows.rs    # FindFirstFileExW + LARGE_FETCH
│   │   │   └── unix.rs       # jwalk (parallel) + walkdir fallback
│   │   └── cache.rs          # redb cache: dir fingerprint = mtime
│   ├── rules/                # Classification engine
│   │   ├── mod.rs            # Junk / Stale / Large / Protected + duplicates
│   │   └── duplicates.rs     # Size grouping + Blake3 (parallel)
│   ├── cleaner/              # Deletion
│   │   └── mod.rs            # trash crate (cross-platform) + hard delete
│   ├── config/               # TOML + env overrides
│   └── server/               # Web server (axum) + embedded assets
│       ├── mod.rs            # HTTP API: /scan, /stop, /progress, /results, /delete, /export
│       └── web/              # HTML/CSS/JS (rust-embed)
├── web/                      # Source web assets
│   ├── index.html
│   ├── style.css
│   └── app.js
├── Cargo.toml
└── config.toml               # Example config
```

### Performance Highlights

1. **Windows**: `FindFirstFileExW` with `FIND_FIRST_EX_LARGE_FETCH` — batch directory reads, reduces syscalls dramatically (critical for WinSxS, node_modules).
2. **Unix**: `jwalk` parallel walker with `walkdir` fallback — uses all cores efficiently.
3. **Work-queue + worker pool** — shared directory queue, N workers. Backpressure handling prevents deadlock on deep trees.
4. **Incremental cache** — directory fingerprint = `mtime`. Unchanged dirs replayed from redb without re-walk. Batched writes via `db.batch()`.
5. **Batch record flush** — mutex acquired per 256 files, not per file.
6. **Parallel duplicate hashing** — Blake3 on all cores (Rayon pool).
7. **Live recent files** — last 50 processed paths streamed to UI.

> **Benchmarked**: ~80k files/sec on NVMe (Windows 11, 12 cores). Cache re-scan: ~5ms for 100k cached files.

## Testing

```bash
# Unit tests (scanner, rules, protection, cache)
cargo test

# Benchmark
cargo run --release -- bench --files 100000 --depth 4 --serial
```

## Example JSON Output

```json
[
  {
    "path": "/home/user/huge.iso",
    "size_bytes": 4294967296,
    "category": "huge",
    "reason": "very large file (> 500 MiB)",
    "risk": "caution",
    "mod_time": "2024-01-15T10:30:00Z"
  },
  {
    "path": "/tmp/setup.tmp",
    "size_bytes": 1048576,
    "category": "junk",
    "reason": "extension .tmp",
    "risk": "safe",
    "mod_time": "2024-01-10T14:22:00Z"
  }
]
```

## Safety

- **Default**: Recycle Bin/Trash only. Recoverable from system trash.
- **Protected paths** (OS directories, pagefile, hiberfile, boot) **never** appear in results unless `protect_system = false`.
- **Hard delete** requires clicking "Delete Permanently" + modal confirmation with warning.
- **Access errors** (permission denied, etc.) are logged; scan continues.

## License

MIT — use, modify, distribute freely.

## Acknowledgments

- `jwalk` / `walkdir` — fast parallel directory traversal
- `redb` — embedded database for incremental cache
- `blake3` — ultra-fast hashing for duplicate detection
- `trash` — cross-platform recycle bin/trash API
- `axum` / `tokio` — async web server
- `rust-embed` — asset embedding
- `clap` — CLI parsing
- `serde` / `toml` — configuration