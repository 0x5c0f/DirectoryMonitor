# Directory Monitor

A cross-platform filesystem monitoring tool written in Rust. Inspired by [DirectoryMonitor](https://directorymonitor.com) for Windows.

English | [中文](README.zh_CN.md)

## Features

### Core Monitoring

- **Real-time detection** — File creation, modification, deletion, renames, access, and metadata changes
- **Detailed event types** — inotifywait-compatible: `CREATE`, `MODIFY`, `ATTRIB`, `CLOSE_WRITE`, `OPEN`, `MOVED_FROM`, `MOVED_TO`, `DELETE`, `ACCESS`
- **Cross-platform** — Windows, Linux, macOS (via `notify` crate)
- **Recursive watching** — Monitor directories and all subdirectories
- **Event filtering** — Include/exclude glob patterns and event type filters per watch
- **Event deduplication** — By (path, event_type) within a configurable time window
- **Batch processing** — Configurable batch size and timeout for downstream consumers
- **Debouncing** — 200ms debounce to coalesce rapid filesystem changes

### Storage & Notifications

- **SQLite persistence** — WAL mode for high-throughput event storage with historical query support
- **Email alerts** — SMTP notifications with batching, throttling, and per-watch recipients
- **Syslog** — RFC 3164 / RFC 5424 support with configurable facility
- **Script execution** — Run custom scripts on events (sync or async mode)
- **Macro system** — Context macros (`%file%`, `%path%`, `%event%`, `%timestamp%`, etc.) for logs and scripts

### Web Dashboard

- **Real-time event stream** — WebSocket-based live event updates
- **Event log** — Paginated history with type, target, and time range filtering
- **Dashboard charts** — Event rate trends, type distribution, and watch root breakdown
- **Configuration editor** — Edit global settings and watch rules via the browser
- **Watcher management** — View active watchers, reload configuration without restart
- **Authentication** — Optional password protection for the web interface
- **Theme support** — Light/dark mode with system preference detection
- **Responsive design** — Optimized for desktop, tablet, and mobile screens

### Metrics & Observability

- **Prometheus export** — `/metrics` endpoint compatible with Prometheus scraping
- **Chart data API** — `/api/metrics/chart` for frontend visualization
- **Time-series windows** — 1-hour per-minute and 7-day per-hour event rate tracking
- **System gauges** — Uptime, active watchers, database size, queue depth
- **Notification stats** — Sent/failed counts by notification type

### Operations

- **Directory snapshots** — Detect changes during network outages or power failures
- **CLI interface** — `run`, `serve`, `validate`, `snapshot` subcommands
- **Make targets** — Build, test, lint, package, install/uninstall

## Quick Start

```bash
# Build
cargo build --release

# Copy and edit the example config
cp config.example.toml config.toml

# Validate configuration
./target/release/directory-monitor -c config.toml validate

# Start monitoring (CLI mode)
./target/release/directory-monitor -c config.toml run

# Or start with web dashboard
./target/release/directory-monitor -c config.toml serve
```

### Install

```bash
make install       # Install to ~/.local/bin
make uninstall     # Remove
```

## Usage

### CLI Mode

```bash
# Default (uses config.tom in current directory)
./directory-monitor run

# Specify config file and log level
./directory-monitor -c /path/to/config.toml -l debug run
```

Events are printed to stdout (or a log file if configured).

### Web Dashboard Mode

```bash
# Default: http://127.0.0.1:8080
./directory-monitor serve

# Custom bind address (e.g., LAN access)
./directory-monitor serve -b 0.0.0.0:9090
```

Open the address in a browser to access the dashboard. Features:

- **Events tab** — Live event stream with filtering by type, watch target, and time range
- **Dashboard tab** — Charts showing event rates, type distribution, and system status
- **Settings tab** — Edit global configuration and manage watch rules
- **Watchers tab** — View active watchers and reload configuration

### Prometheus Monitoring

```bash
# Scrape metrics
curl http://127.0.0.1:8080/metrics
```

Example output:

```
# HELP dm_events_total Total filesystem events by type
# TYPE dm_events_total counter
dm_events_total{type="CREATE"} 42
dm_events_total{type="MODIFY"} 128

# HELP dm_active_watchers Number of active file system watchers
# TYPE dm_active_watchers gauge
dm_active_watchers 3

# HELP dm_uptime_seconds Process uptime in seconds
# TYPE dm_uptime_seconds gauge
dm_uptime_seconds 3600
```

## Configuration

See [`config.example.toml`](config.example.toml) for a fully annotated example.

### Minimal Example

```toml
[[watches]]
path = "/home/user/documents"
recursive = true
include = ["*.txt", "*.doc", "*.pdf"]
exclude = ["*.tmp", "~*"]
event_types = ["created", "modified", "deleted"]
```

### Event Types

Event type names are case-insensitive and support multiple formats:

| Event | Aliases | Description |
|-------|---------|-------------|
| `CREATE` | `created` | File/directory created |
| `MODIFY` | `modified` | File content written |
| `ATTRIB` | | Metadata changed (permissions, timestamps) |
| `CLOSE_WRITE` | | Closed after write (most reliable write-complete signal) |
| `CLOSE_NOWRITE` | | Closed read-only |
| `OPEN` | | File opened |
| `MOVED_TO` | | File moved into watch directory |
| `MOVED_FROM` | | File moved out of watch directory |
| `DELETE` | `deleted` | File/directory deleted |
| `RENAME` | `renamed` | File renamed |
| `ACCESS` | | File content read |

### Log Format Placeholders

| Placeholder | Description |
|-------------|-------------|
| `%file%` | File name |
| `%directory%` | Parent directory path |
| `%event%` | Event type (e.g., `CREATE`) |
| `%path%` | Full path |
| `%target%` | Rename target path (rename events only) |
| `%timestamp%` | Event timestamp (RFC 3339) |
| `%user%` | Operating user (PRO feature) |
| `%process%` | Operating process (PRO feature) |

## Architecture

```
Filesystem → notify → FsWatcher → [debounce 200ms] → WatchEvent
    → broadcast channel → multiple consumers:
        → EventProcessor (filter → dedup → batch)
        → EventStore (SQLite)
        → Notifiers (email, syslog, scripts)
        → Web Server (WebSocket real-time push)
        → MetricsRegistry (counters, time-series, Prometheus)
```

Events flow through `tokio::sync::broadcast` channels, allowing multiple consumers to process events concurrently.

## Project Structure

```
crates/
├── dm-core/          Core types: events, config, errors
├── dm-watcher/       Filesystem monitoring engine (notify wrapper + debounce)
├── dm-processor/     Event filtering, deduplication, batch processing
├── dm-storage/       SQLite event persistence (WAL mode)
├── dm-notify/        Notification system (email, syslog, scripts)
├── dm-metrics/       Metrics collection and Prometheus export
├── dm-web/           Web dashboard (Axum + WebSocket + HTML/JS/CSS)
└── dm-cli/           CLI entry point
```

## Make Commands

```bash
make              # Build release (default)
make build        # Build debug
make test         # Run tests
make lint         # Run clippy
make clean        # Clean build artifacts
make dist         # Create distribution tarball
make install      # Install to ~/.local/bin
make uninstall    # Remove from ~/.local/bin
make run          # Run with example config
make validate     # Validate example config
make help         # Show all commands
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Web dashboard |
| `GET` | `/ws` | WebSocket event stream |
| `GET` | `/api/events` | Query historical events |
| `GET` | `/api/config` | Get current configuration |
| `PUT` | `/api/config/global` | Update global settings |
| `POST` | `/api/config/watches` | Add a watch rule |
| `PUT` | `/api/config/watches/{idx}` | Update a watch rule |
| `DELETE` | `/api/config/watches/{idx}` | Delete a watch rule |
| `GET` | `/api/watchers` | List active watchers |
| `POST` | `/api/watchers/reload` | Reload configuration |
| `GET` | `/api/auth/status` | Authentication status |
| `POST` | `/api/auth/login` | Authenticate |
| `GET` | `/api/auth/verify` | Verify session |
| `GET` | `/metrics` | Prometheus metrics |
| `GET` | `/api/metrics/chart` | Chart data (JSON) |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `notify` v7 | Cross-platform filesystem monitoring |
| `rusqlite` | SQLite event storage |
| `lettre` | SMTP email notifications |
| `tokio` | Async runtime |
| `axum` | Web server framework |
| `tracing` | Structured logging |
| `clap` | CLI argument parsing |
| `globset` | Glob pattern matching |
| `serde` / `toml` | Configuration serialization |
| `chrono` | Time handling |
| `uuid` | Unique identifiers |

## Requirements

- Rust 1.75+
- Linux, macOS, or Windows

## License

MIT
