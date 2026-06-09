# Rust Repository Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将当前 Rust workspace 调整到可通过 rustfmt、Clippy、测试和发布 CI 的规范状态，并降低后续维护中的结构、错误处理和运行时阻塞风险。

**Architecture:** 先修复会直接阻断 CI 的小问题，再统一 Cargo workspace 元数据和依赖声明。随后逐步收敛错误类型、存储执行模型、Web/CLI 大文件边界，避免一次性大重构导致行为回归。

**Tech Stack:** Rust 2021, Cargo workspace, rustfmt, Clippy, Tokio, Axum, rusqlite, notify, thiserror, anyhow.

---

## 当前状态摘要

评审时已运行以下命令：

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

结果：

- `cargo fmt --check` 失败，集中在 `crates/dm-cli/src/windows_service.rs`。
- `cargo clippy --workspace --all-targets -- -D warnings` 失败，集中在 `crates/dm-core/tests/config.rs` 的 bool assert 写法。
- `cargo test --workspace` 通过，普通测试共 166 个通过，doc-test 通过。
- 当前 workspace 结构总体合理，主要问题是质量门禁、manifest 继承、错误处理、同步 SQLite 操作在 async 上下文执行，以及 `dm-web`/`dm-cli` 大文件职责过多。

## 文件结构规划

本计划按阶段改动以下文件：

- 修改：`crates/dm-cli/src/windows_service.rs`，修复 rustfmt 格式问题。
- 修改：`crates/dm-core/tests/config.rs`，修复 Clippy bool assert。
- 修改：`Cargo.toml`，补齐 workspace dev-dependencies，必要时补充质量命令说明。
- 修改：`crates/*/Cargo.toml`，统一 `rust-version.workspace = true` 和 workspace 依赖。
- 修改：`crates/dm-web/Cargo.toml`，改为继承 workspace package 和 dependencies。
- 修改：`Makefile`，让 lint 覆盖 `--workspace --all-targets`，新增 `fmt-check` 或 `check` 目标。
- 修改：`.github/workflows/release.yml`，补充 `cargo fmt --check`，并让 Clippy 使用 `--workspace --all-targets`。
- 修改：`crates/dm-notify/src/syslog.rs`，将 panic 型构造改为 `Result`。
- 修改：`crates/dm-cli/src/main.rs`，适配 syslog 构造错误、修正 validate 语义，并逐步拆分运行编排。
- 修改：`crates/dm-storage/src/sqlite.rs`，为同步 rusqlite 操作建立阻塞隔离或专用 worker。
- 修改：`crates/dm-web/src/server.rs`，拆分 auth、events、config、watchers、metrics 路由模块。
- 新建：`crates/dm-web/src/auth.rs`。
- 新建：`crates/dm-web/src/routes/events.rs`。
- 新建：`crates/dm-web/src/routes/config.rs`。
- 新建：`crates/dm-web/src/routes/watchers.rs`。
- 新建：`crates/dm-web/src/routes/metrics.rs`。
- 新建：`crates/dm-cli/src/runner.rs`。
- 新建：`crates/dm-cli/src/pipeline.rs`。
- 可选新建：`crates/dm-storage/src/query.rs`，封装查询参数，减少 `too_many_arguments`。

---

## Task 1: 修复阻断 CI 的 rustfmt 和 Clippy 问题

**Files:**

- Modify: `crates/dm-cli/src/windows_service.rs`
- Modify: `crates/dm-core/tests/config.rs`

- [ ] **Step 1: 运行格式化检查确认当前失败**

Run:

```bash
cargo fmt --check
```

Expected: FAIL，输出包含 `crates/dm-cli/src/windows_service.rs` 的 diff。

- [ ] **Step 2: 应用 rustfmt**

Run:

```bash
cargo fmt
```

Expected: 命令退出码为 0，`windows_service.rs` 被格式化。

- [ ] **Step 3: 修复 bool assert Clippy 问题**

将 `crates/dm-core/tests/config.rs` 中：

```rust
assert_eq!(config.database.enabled, true);
```

改为：

```rust
assert!(config.database.enabled);
```

将：

```rust
assert_eq!(config2.watches[0].recursive, false);
```

改为：

```rust
assert!(!config2.watches[0].recursive);
```

- [ ] **Step 4: 验证格式和 lint**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: 三个命令均退出码 0。

- [ ] **Step 5: 提交**

```bash
git add crates/dm-cli/src/windows_service.rs crates/dm-core/tests/config.rs
git commit -m "chore: fix rustfmt and clippy violations"
```

---

## Task 2: 统一 Cargo workspace 元数据和依赖声明

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/dm-core/Cargo.toml`
- Modify: `crates/dm-watcher/Cargo.toml`
- Modify: `crates/dm-processor/Cargo.toml`
- Modify: `crates/dm-storage/Cargo.toml`
- Modify: `crates/dm-notify/Cargo.toml`
- Modify: `crates/dm-metrics/Cargo.toml`
- Modify: `crates/dm-web/Cargo.toml`
- Modify: `crates/dm-cli/Cargo.toml`

- [ ] **Step 1: 确认 rust-version 未继承**

Run:

```bash
cargo metadata --no-deps --format-version 1 | jq '.packages[] | {name, rust_version}'
```

Expected: 当前各成员包的 `rust_version` 为 `null`。

- [ ] **Step 2: 给所有成员 crate 添加 rust-version 继承**

在每个 `crates/*/Cargo.toml` 的 `[package]` 中加入：

```toml
rust-version.workspace = true
```

示例：

```toml
[package]
name = "dm-core"
description = "Core types and configuration for Directory Monitor"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
```

- [ ] **Step 3: 将 dm-web 改为 workspace 风格**

将 `crates/dm-web/Cargo.toml` 改为：

```toml
[package]
name = "dm-web"
description = "Web server for Directory Monitor"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
axum = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
uuid = { workspace = true }
dm-core = { workspace = true }
dm-metrics = { workspace = true }
dm-processor = { workspace = true }
dm-storage = { workspace = true }
dm-watcher = { workspace = true }

[dev-dependencies]
tower = { workspace = true }
http-body-util = { workspace = true }
toml = { workspace = true }
tempfile = { workspace = true }
```

- [ ] **Step 4: 在根 Cargo.toml 补齐 dev 依赖**

在 `Cargo.toml` 的 `[workspace.dependencies]` 中加入：

```toml
tempfile = "3"
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
parking_lot = "0.12"
```

同时把 `crates/dm-metrics/Cargo.toml` 中：

```toml
parking_lot = "0.12"
```

改为：

```toml
parking_lot = { workspace = true }
```

- [ ] **Step 5: 验证 metadata 和构建**

Run:

```bash
cargo metadata --no-deps --format-version 1 | jq '.packages[] | {name, rust_version}'
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: 各成员包 `rust_version` 均为 `"1.75"`，测试和 Clippy 通过。

- [ ] **Step 6: 提交**

```bash
git add Cargo.toml crates/*/Cargo.toml
git commit -m "chore: normalize workspace manifests"
```

---

## Task 3: 强化本地质量门禁和 CI

**Files:**

- Modify: `Makefile`
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: 更新 Makefile lint 范围**

将 `Makefile` 中：

```make
lint:
	cargo clippy -- -D warnings
```

改为：

```make
lint:
	cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 2: 新增 fmt-check 和 check 目标**

将 `.PHONY` 改为包含 `fmt-check check`：

```make
.PHONY: all build release test fmt-check lint check clean dist install uninstall run validate help
```

新增：

```make
## Check rustfmt formatting
fmt-check:
	cargo fmt --check

## Run all local quality checks
check: fmt-check lint test
```

- [ ] **Step 3: 更新 release workflow**

在 `.github/workflows/release.yml` 的 test job 中，将步骤调整为：

```yaml
      - name: Check formatting
        run: cargo fmt --check
      - name: Run tests
        run: cargo test --workspace
      - name: Run clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 4: 本地验证**

Run:

```bash
make check
```

Expected: `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test` 均通过。

- [ ] **Step 5: 提交**

```bash
git add Makefile .github/workflows/release.yml
git commit -m "ci: enforce fmt and workspace clippy checks"
```

---

## Task 4: 改善 syslog 构造错误处理，移除生产代码 panic

**Files:**

- Modify: `crates/dm-notify/src/syslog.rs`
- Modify: `crates/dm-cli/src/main.rs`

- [ ] **Step 1: 添加失败构造测试**

在 `crates/dm-notify/src/syslog.rs` 添加测试模块，测试 facility 解析保持默认 user。因为 UDP bind 失败很难稳定触发，先覆盖构造成功和格式路径：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use dm_core::config::SyslogConfig;

    fn config() -> SyslogConfig {
        SyslogConfig {
            enabled: true,
            server: "127.0.0.1".to_string(),
            port: 514,
            format: "rfc5424".to_string(),
            facility: "unknown".to_string(),
            message_format: None,
        }
    }

    #[test]
    fn new_returns_result() {
        let notifier = SyslogNotifier::new(&config());
        assert!(notifier.is_ok());
    }
}
```

- [ ] **Step 2: 将构造函数改为 Result**

将：

```rust
pub fn new(config: &SyslogConfig) -> Self {
```

改为：

```rust
pub fn new(config: &SyslogConfig) -> Result<Self, String> {
```

将：

```rust
let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP socket");
```

改为：

```rust
let socket = UdpSocket::bind("0.0.0.0:0")
    .map_err(|e| format!("Failed to bind UDP socket: {e}"))?;
```

将返回值包为：

```rust
Ok(Self {
    config: SyslogNotifierConfig {
        server: config.server.clone(),
        port: config.port,
        format: config.format.clone(),
        facility,
        message_format: config.message_format.clone(),
    },
    socket,
})
```

- [ ] **Step 3: 修改 CLI 调用点**

在 `crates/dm-cli/src/main.rs` 中，将：

```rust
let syslog_notifier = if config.notifications.syslog.enabled {
    Some(dm_notify::SyslogNotifier::new(&config.notifications.syslog))
} else {
    None
};
```

改为：

```rust
let syslog_notifier = if config.notifications.syslog.enabled {
    Some(
        dm_notify::SyslogNotifier::new(&config.notifications.syslog)
            .map_err(|e| anyhow::anyhow!("Failed to initialize syslog notifier: {e}"))?,
    )
} else {
    None
};
```

- [ ] **Step 4: 验证**

Run:

```bash
cargo test -p dm-notify
cargo test -p dm-cli
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: 测试和 Clippy 均通过。

- [ ] **Step 5: 提交**

```bash
git add crates/dm-notify/src/syslog.rs crates/dm-cli/src/main.rs
git commit -m "refactor: return errors from syslog notifier initialization"
```

---

## Task 5: 修正 validate 命令语义

**Files:**

- Modify: `crates/dm-cli/src/main.rs`

- [ ] **Step 1: 为 validate_config 添加单元测试入口**

将：

```rust
fn validate_config(config: &AppConfig) -> Result<()> {
```

保持为私有函数即可，在同文件底部添加测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use dm_core::config::{AppConfig, WatchConfig};

    #[test]
    fn validate_config_fails_for_missing_watch_path() {
        let mut config = AppConfig::default();
        config.watches.push(WatchConfig {
            path: PathBuf::from("/definitely/missing/directory-monitor-test-path"),
            recursive: true,
            include: vec![],
            exclude: vec![],
            event_types: vec![],
            log_file: None,
            log_format: None,
            script: None,
            script_mode: "async".to_string(),
            email_recipients: vec![],
        });

        let result = validate_config(&config);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run:

```bash
cargo test -p dm-cli validate_config_fails_for_missing_watch_path
```

Expected: FAIL，因为当前 `validate_config` 总是返回 `Ok(())`。

- [ ] **Step 3: 实现错误返回**

将 `validate_config` 中 watch path 检查改为收集错误：

```rust
let mut missing_paths = Vec::new();

for watch in &config.watches {
    if !watch.path.exists() {
        error!("Watch path does not exist: {}", watch.path.display());
        missing_paths.push(watch.path.display().to_string());
    } else {
        info!("  {}", watch.path.display());
    }
}

if !missing_paths.is_empty() {
    anyhow::bail!("Missing watch paths: {}", missing_paths.join(", "));
}
```

保留 email、syslog、database 的 info 输出。

- [ ] **Step 4: 验证**

Run:

```bash
cargo test -p dm-cli validate_config_fails_for_missing_watch_path
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: 全部通过。

- [ ] **Step 5: 提交**

```bash
git add crates/dm-cli/src/main.rs
git commit -m "fix: make config validation fail on missing watch paths"
```

---

## Task 6: 降低 SQLite 同步操作阻塞 Tokio 的风险

**Files:**

- Modify: `crates/dm-storage/src/sqlite.rs`
- Optional Create: `crates/dm-storage/src/query.rs`
- Modify: `crates/dm-storage/src/lib.rs`
- Modify: `crates/dm-storage/tests/storage.rs`

- [ ] **Step 1: 新建查询参数类型**

创建 `crates/dm-storage/src/query.rs`：

```rust
#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    pub limit: usize,
    pub offset: usize,
    pub event_types: Vec<String>,
    pub watch_root: Option<String>,
    pub search: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub is_dir: Option<bool>,
}

impl EventQuery {
    pub fn page(limit: usize, offset: usize) -> Self {
        Self {
            limit,
            offset,
            ..Self::default()
        }
    }
}
```

在 `crates/dm-storage/src/lib.rs` 增加：

```rust
pub mod query;
pub use query::EventQuery;
```

- [ ] **Step 2: 添加新 query API 测试**

在 `crates/dm-storage/tests/storage.rs` 增加：

```rust
use dm_storage::EventQuery;

#[tokio::test]
async fn test_query_with_event_query_struct() {
    let store = EventStore::open_memory().unwrap();
    let event = make_event(EventType::Created, "/tmp/query-struct.txt");
    store.insert(&event).await.unwrap();

    let query = EventQuery {
        limit: 10,
        offset: 0,
        event_types: vec!["CREATE".to_string()],
        ..EventQuery::default()
    };

    let events = store.query_events(query).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].path, PathBuf::from("/tmp/query-struct.txt"));
}
```

- [ ] **Step 3: 实现 query_events 并保留旧 query 包装**

在 `EventStore` 中新增：

```rust
pub async fn query_events(&self, query: crate::EventQuery) -> Result<Vec<FsEvent>, StorageError> {
    self.query(
        query.limit,
        query.offset,
        &query.event_types,
        query.watch_root.as_deref(),
        query.search.as_deref(),
        query.after.as_deref(),
        query.before.as_deref(),
        query.is_dir,
    )
    .await
}
```

这一步先降低 API 复杂度，不改变执行模型。

- [ ] **Step 4: 评估执行模型改造方式**

优先选择专用 DB worker，而不是在每个方法里 `spawn_blocking` 后继续共享同一个 connection。原因：`rusqlite::Connection` 的所有权和串行访问更清晰，写入顺序可控。

最小落地方式：

- `EventStore` 持有 `tokio::sync::mpsc::Sender<DbCommand>`。
- worker 在线程内持有 `rusqlite::Connection`。
- 每个 async 方法通过 oneshot 接收结果。

如果当前迭代只想小步前进，先完成 `EventQuery`，后续单独计划 DB worker。

- [ ] **Step 5: 验证**

Run:

```bash
cargo test -p dm-storage
cargo clippy -p dm-storage --all-targets -- -D warnings
```

Expected: storage 测试和 Clippy 通过。

- [ ] **Step 6: 提交**

```bash
git add crates/dm-storage/src/lib.rs crates/dm-storage/src/query.rs crates/dm-storage/src/sqlite.rs crates/dm-storage/tests/storage.rs
git commit -m "refactor: introduce storage query parameters"
```

---

## Task 7: 拆分 dm-web server 大文件

**Files:**

- Modify: `crates/dm-web/src/lib.rs`
- Modify: `crates/dm-web/src/server.rs`
- Create: `crates/dm-web/src/auth.rs`
- Create: `crates/dm-web/src/routes/mod.rs`
- Create: `crates/dm-web/src/routes/events.rs`
- Create: `crates/dm-web/src/routes/config.rs`
- Create: `crates/dm-web/src/routes/watchers.rs`
- Create: `crates/dm-web/src/routes/metrics.rs`

- [ ] **Step 1: 先只移动 auth 逻辑**

创建 `crates/dm-web/src/auth.rs`，移动以下函数：

```rust
pub async fn check_auth(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode>
pub fn extract_token(headers: &HeaderMap) -> Option<String>
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool
```

其中 `check_auth` 引用：

```rust
use axum::http::{HeaderMap, StatusCode};
use crate::server::AppState;
```

在 `server.rs` 中改为：

```rust
use crate::auth::check_auth;
```

在 `lib.rs` 中加入：

```rust
mod auth;
```

- [ ] **Step 2: 移动 auth 测试**

把 `test_extract_token_*` 和 `test_constant_time_eq_*` 从 `server.rs` 移到 `auth.rs` 的测试模块。测试函数内容保持不变。

- [ ] **Step 3: 验证 auth 拆分**

Run:

```bash
cargo test -p dm-web test_extract_token_valid
cargo test -p dm-web test_constant_time_eq_identical
cargo clippy -p dm-web --all-targets -- -D warnings
```

Expected: 通过。

- [ ] **Step 4: 创建 routes 模块**

创建 `crates/dm-web/src/routes/mod.rs`：

```rust
pub mod config;
pub mod events;
pub mod metrics;
pub mod watchers;
```

在 `lib.rs` 中加入：

```rust
mod routes;
```

- [ ] **Step 5: 逐个移动路由处理器**

按下面顺序移动，每移动一个模块就运行对应测试：

1. `metrics_prometheus_handler`、`metrics_chart_handler` -> `routes/metrics.rs`
2. `events_handler` -> `routes/events.rs`
3. `config_get_handler`、`config_put_watch_handler`、`config_add_watch_handler`、`config_delete_watch_handler`、`config_put_global_handler` -> `routes/config.rs`
4. `watchers_list_handler`、`watchers_reload_handler` -> `routes/watchers.rs`

需要公开给 `build_router` 使用的 handler 用 `pub(crate)`：

```rust
pub(crate) async fn events_handler(...) -> Result<..., StatusCode> {
    ...
}
```

- [ ] **Step 6: 验证 Web API**

Run:

```bash
cargo test -p dm-web
cargo clippy -p dm-web --all-targets -- -D warnings
```

Expected: 所有 dm-web 单元测试和 API 集成测试通过。

- [ ] **Step 7: 提交**

```bash
git add crates/dm-web/src
git commit -m "refactor: split web server handlers by responsibility"
```

---

## Task 8: 拆分 dm-cli 运行编排和事件处理

**Files:**

- Modify: `crates/dm-cli/src/main.rs`
- Create: `crates/dm-cli/src/runner.rs`
- Create: `crates/dm-cli/src/pipeline.rs`

- [ ] **Step 1: 创建 pipeline 模块**

创建 `crates/dm-cli/src/pipeline.rs`，移动：

```rust
type MonitorComponents = (...)
fn setup_monitoring(config: &AppConfig) -> Result<MonitorComponents>
async fn process_watch_event(...)
```

将需要被 `runner.rs` 调用的函数改为：

```rust
pub(crate) fn setup_monitoring(config: &AppConfig) -> Result<MonitorComponents>
pub(crate) async fn process_watch_event(...)
```

在 `main.rs` 添加：

```rust
mod pipeline;
mod runner;
```

- [ ] **Step 2: 创建 runner 模块**

创建 `crates/dm-cli/src/runner.rs`，移动：

```rust
pub(crate) async fn run_monitor(config: AppConfig) -> Result<()>
pub(crate) async fn run_serve(mut config: AppConfig, config_path: PathBuf, bind: &Option<String>) -> Result<()>
pub(crate) fn take_snapshot(path: &Path, output: &Path) -> Result<()>
```

在 `main.rs` 中改为调用：

```rust
use runner::{run_monitor, run_serve, take_snapshot};
```

- [ ] **Step 3: 保持 Windows service 可访问运行函数**

`windows_service.rs` 当前使用：

```rust
use crate::{run_monitor, run_serve};
```

拆分后改为：

```rust
use crate::runner::{run_monitor, run_serve};
```

并确保 `runner` 模块在 `main.rs` 中声明为：

```rust
pub(crate) mod runner;
```

- [ ] **Step 4: 验证 CLI**

Run:

```bash
cargo test -p dm-cli
cargo clippy -p dm-cli --all-targets -- -D warnings
cargo build -p dm-cli
```

Expected: 测试、Clippy、构建均通过。

- [ ] **Step 5: 提交**

```bash
git add crates/dm-cli/src/main.rs crates/dm-cli/src/runner.rs crates/dm-cli/src/pipeline.rs crates/dm-cli/src/windows_service.rs
git commit -m "refactor: split cli runner and event pipeline"
```

---

## Task 9: 最终全仓库验证

**Files:**

- No direct code changes unless previous tasks expose failures.

- [ ] **Step 1: 运行全套质量门禁**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Expected: 全部退出码 0。

- [ ] **Step 2: 检查工作区状态**

Run:

```bash
git status --short
```

Expected: 只包含本计划任务产生的预期文件改动；没有 `target/`、`dist/`、本地数据库、`config.toml` 入库。

- [ ] **Step 3: 记录优化结果**

在 PR 或最终说明中写明：

```text
Verification:
- cargo fmt --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- cargo build --workspace
```

- [ ] **Step 4: 提交最终整理**

如果前面任务没有单独提交，最后统一提交：

```bash
git add Cargo.toml Makefile .github/workflows/release.yml crates
git commit -m "chore: improve rust workspace quality and structure"
```

---

## 推荐执行顺序

1. 先执行 Task 1-3，目标是让格式化、Clippy、测试和 CI 门禁稳定。
2. 再执行 Task 4-5，目标是修正明显的错误处理和 validate 语义问题。
3. 然后执行 Task 6，先引入 `EventQuery`，DB worker 可根据实际负载另开后续任务。
4. 最后执行 Task 7-8，拆分大文件。拆分时每移动一个模块就运行一次局部测试，避免回归堆积。
5. 完成后执行 Task 9。

## 风险和约束

- `dm-web/src/server.rs` 拆分时要保持现有路由路径完全不变，尤其是 `/api/config/watches/{idx}`、`/api/events`、`/metrics`。
- `dm-cli` 拆分时要保留 Windows service 编译路径，Linux 本机不一定能覆盖 Windows 专属代码，CI 的 Windows target 构建仍然重要。
- SQLite 执行模型改造有行为风险，不建议和 Web/CLI 拆分混在同一个提交中。
- 不要在同一次优化里改前端模板样式；这份计划只处理 Rust 代码规范、项目结构和运行时风险。

## 完成标准

仓库达到以下状态才算本轮优化完成：

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

四个命令全部通过；`cargo metadata` 显示所有 workspace 成员继承 `rust_version = "1.75"`；`dm-web` 和 `dm-cli` 的核心大文件已按职责拆分，且 API/CLI 行为测试仍通过。
