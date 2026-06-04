# Directory Monitor (Rust)

A cross-platform filesystem monitoring tool, written in Rust. Inspired by [DirectoryMonitor](https://directorymonitor.com) for Windows.

## Features

- **Real-time monitoring** — Detects file creation, modification, deletion, renames, and access
- **Detailed event types** — inotifywait-compatible categories: `CREATE`, `MODIFY`, `ATTRIB`, `CLOSE_WRITE`, `OPEN`, `MOVED_FROM`, `MOVED_TO`, `DELETE`, `ACCESS`
- **Cross-platform** — Supports Windows, Linux, and macOS (via `notify` crate)
- **Event filtering** — Include/exclude glob patterns per watched directory
- **Event deduplication** — Deduplicates by (path, event_type) to preserve distinct event types
- **Batch processing** — Configurable batch size and timeout for downstream consumers
- **SQLite storage** — WAL mode for high-throughput event persistence
- **Notifications** — Email (SMTP), Syslog (RFC3164/5424), script execution, sound alerts
- **Macro system** — Context macros (`%file%`, `%path%`, `%event%`, etc.) for logs and scripts
- **Directory snapshots** — Detect changes during network outages or power failures
- **CLI interface** — Full command-line tool with run, validate, and snapshot commands

## Project Structure

```
crates/
├── dm-core/          Core types: events, config, errors
├── dm-watcher/       Filesystem monitoring engine (notify wrapper)
├── dm-processor/     Event filtering, deduplication, batching
├── dm-storage/       SQLite event persistence
├── dm-notify/        Notification system (email, syslog, scripts)
└── dm-cli/           CLI entry point
```

## Quick Start

```bash
# Build
cargo build --release

# Copy and edit the example config
cp config.example.toml config.toml
# Edit config.toml to add your watch directories

# Validate configuration
./target/release/directory-monitor -c config.toml validate

# Start monitoring
./target/release/directory-monitor -c config.toml run
```

## Configuration

See [`config.example.toml`](config.example.toml) for a full annotated example.

## Architecture

The project uses a modular architecture with async event processing:

```
Filesystem → notify → FsWatcher → [debounce] → WatchEvent
    → EventProcessor (filter → dedup → batch)
        → EventStore (SQLite)
        → Notifiers (email, syslog, scripts)
```

Events flow through `tokio::sync::mpsc` channels, allowing multiple consumers to process events concurrently.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `notify` v7 | Cross-platform filesystem monitoring |
| `rusqlite` | SQLite event storage |
| `lettre` | SMTP email notifications |
| `tokio` | Async runtime |
| `tracing` | Structured logging |
| `clap` | CLI argument parsing |
| `globset` | Glob pattern matching |
| `serde` / `toml` | Configuration serialization |

## License

MIT
