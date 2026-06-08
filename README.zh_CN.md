# Directory Monitor

跨平台文件系统监控工具，使用 Rust 编写。灵感来源于 Windows 平台的 [DirectoryMonitor](https://directorymonitor.com)。

[English](README.md) | 中文

## 功能特性

### 核心监控

- **实时检测** — 文件创建、修改、删除、重命名、访问及元数据变更
- **详细事件类型** — 对齐 inotifywait 风格：`CREATE`、`MODIFY`、`ATTRIB`、`CLOSE_WRITE`、`OPEN`、`MOVED_FROM`、`MOVED_TO`、`DELETE`、`ACCESS`
- **跨平台** — 支持 Windows、Linux、macOS（基于 `notify` crate）
- **递归监控** — 监控目录及其所有子目录
- **事件过滤** — 按 glob 模式（`include`/`exclude`）和事件类型（`event_types`）筛选
- **事件去重** — 按（路径, 事件类型）在可配置时间窗口内去重
- **批量处理** — 可配置批量大小和超时时间
- **防抖处理** — 200ms 防抖合并快速文件系统变更

### 存储与通知

- **SQLite 持久化** — WAL 模式，高吞吐事件存储，支持历史查询
- **邮件告警** — SMTP 通知，支持批量发送、节流控制、按目标配置收件人
- **Syslog** — 支持 RFC 3164 / RFC 5424，可配置设施级别
- **脚本执行** — 事件触发时执行自定义脚本（同步或异步模式）
- **宏系统** — 日志格式和脚本参数支持上下文宏（`%file%`、`%path%`、`%event%`、`%timestamp%` 等）

### Web 仪表盘

- **实时事件流** — 基于 WebSocket 的实时事件推送
- **事件日志** — 分页历史记录，支持按类型、监控目标、时间范围筛选
- **仪表盘图表** — 事件速率趋势、类型分布、监控根目录统计
- **配置编辑器** — 在浏览器中编辑全局设置和监控规则
- **监控管理** — 查看活跃监控器，无需重启即可重载配置
- **身份认证** — 可选的密码保护
- **主题切换** — 明亮/暗色模式，自动检测系统偏好
- **响应式设计** — 针对桌面、平板、手机优化

### 指标与可观测性

- **Prometheus 导出** — `/metrics` 端点，兼容 Prometheus 抓取
- **图表数据 API** — `/api/metrics/chart` 提供前端可视化数据
- **时间序列窗口** — 1 小时（每分钟）和 7 天（每小时）事件速率跟踪
- **系统仪表** — 运行时间、活跃监控器、数据库大小、队列深度
- **通知统计** — 按通知类型统计发送/失败次数

### 运维

- **目录快照** — 断网/断电后检测变更
- **CLI 命令行** — `run`、`serve`、`validate`、`snapshot` 子命令
- **Make 命令** — 构建、测试、检查、打包、安装/卸载

## 快速开始

```bash
# 构建
cargo build --release

# 复制并编辑配置文件
cp config.example.toml config.toml

# 验证配置
./target/release/directory-monitor -c config.toml validate

# 开始监控（命令行模式）
./target/release/directory-monitor -c config.toml run

# 或启动 Web 仪表盘
./target/release/directory-monitor -c config.toml serve
```

### 安装

```bash
make install       # 安装到 ~/.local/bin
make uninstall     # 卸载
```

## 使用方式

### 命令行模式

```bash
# 默认（使用当前目录的 config.toml）
./directory-monitor run

# 指定配置文件和日志级别
./directory-monitor -c /path/to/config.toml -l debug run
```

事件输出到终端（或配置的日志文件）。

### Web 仪表盘模式

```bash
# 默认：http://127.0.0.1:8080
./directory-monitor serve

# 自定义绑定地址（如局域网访问）
./directory-monitor serve -b 0.0.0.0:9090
```

在浏览器中打开地址即可访问仪表盘：

- **事件标签页** — 实时事件流，支持按类型、监控目标、时间范围筛选
- **仪表盘标签页** — 事件速率趋势图、类型分布图、系统状态
- **设置标签页** — 编辑全局配置、管理监控规则
- **监控器标签页** — 查看活跃监控器、重载配置

### Prometheus 监控

```bash
# 抓取指标
curl http://127.0.0.1:8080/metrics
```

输出示例：

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

## 配置说明

详见 [`config.example.toml`](config.example.toml)，每个配置项都有中文注释。

### 最小配置示例

```toml
[[watches]]
path = "/home/user/documents"
recursive = true
include = ["*.txt", "*.doc", "*.pdf"]
exclude = ["*.tmp", "~*"]
event_types = ["created", "modified", "deleted"]
```

### 事件类型

事件类型名称不区分大小写，支持多种格式：

| 事件 | 别名 | 说明 |
|------|------|------|
| `CREATE` | `created` | 文件/目录创建 |
| `MODIFY` | `modified` | 文件内容写入 |
| `ATTRIB` | | 元数据变更（权限、时间戳） |
| `CLOSE_WRITE` | | 写入后关闭（最可靠的写完成信号） |
| `CLOSE_NOWRITE` | | 只读后关闭 |
| `OPEN` | | 文件打开 |
| `MOVED_TO` | | 文件移入监控目录 |
| `MOVED_FROM` | | 文件移出监控目录 |
| `DELETE` | `deleted` | 文件/目录删除 |
| `RENAME` | `renamed` | 重命名 |
| `ACCESS` | | 文件内容读取 |

### 日志格式占位符

| 占位符 | 说明 |
|--------|------|
| `%file%` | 文件名 |
| `%directory%` | 父目录路径 |
| `%event%` | 事件类型（如 `CREATE`） |
| `%path%` | 完整路径 |
| `%target%` | 重命名目标路径（仅 rename 事件） |
| `%timestamp%` | 事件时间戳（RFC 3339 格式） |
| `%user%` | 操作用户（PRO 功能） |
| `%process%` | 操作进程（PRO 功能） |

## 架构设计

```
文件系统 → notify → FsWatcher → [防抖 200ms] → WatchEvent
    → broadcast 通道 → 多消费者:
        → EventProcessor (过滤 → 去重 → 批量)
        → EventStore (SQLite)
        → 通知器 (邮件、Syslog、脚本)
        → Web Server (WebSocket 实时推送)
        → MetricsRegistry (计数器、时间序列、Prometheus)
```

事件通过 `tokio::sync::broadcast` 通道分发，支持多个消费者并发处理。

## 项目结构

```
crates/
├── dm-core/          核心类型：事件、配置、错误定义
├── dm-watcher/       文件系统监控引擎（notify 封装 + 防抖）
├── dm-processor/     事件过滤、去重、批量处理
├── dm-storage/       SQLite 事件持久化（WAL 模式）
├── dm-notify/        通知系统（邮件、Syslog、脚本）
├── dm-metrics/       指标收集与 Prometheus 导出
├── dm-web/           Web 仪表盘（Axum + WebSocket + HTML/JS/CSS）
└── dm-cli/           CLI 入口
```

## Make 命令

```bash
make              # 构建 release 版本（默认）
make build        # 构建 debug 版本
make test         # 运行测试
make lint         # 代码检查（clippy）
make clean        # 清理构建产物
make dist         # 打包分发 tarball
make install      # 安装到 ~/.local/bin
make uninstall    # 卸载
make run          # 使用示例配置运行
make validate     # 验证配置文件
make help         # 查看所有命令
```

## API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/` | Web 仪表盘 |
| `GET` | `/ws` | WebSocket 事件流 |
| `GET` | `/api/events` | 查询历史事件 |
| `GET` | `/api/config` | 获取当前配置 |
| `PUT` | `/api/config/global` | 更新全局设置 |
| `POST` | `/api/config/watches` | 添加监控规则 |
| `PUT` | `/api/config/watches/{idx}` | 更新监控规则 |
| `DELETE` | `/api/config/watches/{idx}` | 删除监控规则 |
| `GET` | `/api/watchers` | 列出活跃监控器 |
| `POST` | `/api/watchers/reload` | 重载配置 |
| `GET` | `/api/auth/status` | 认证状态 |
| `POST` | `/api/auth/login` | 登录认证 |
| `GET` | `/api/auth/verify` | 验证会话 |
| `GET` | `/metrics` | Prometheus 指标 |
| `GET` | `/api/metrics/chart` | 图表数据（JSON） |

## 依赖

| Crate | 用途 |
|-------|------|
| `notify` v7 | 跨平台文件系统监控 |
| `rusqlite` | SQLite 事件存储 |
| `lettre` | SMTP 邮件通知 |
| `tokio` | 异步运行时 |
| `axum` | Web 服务器框架 |
| `tracing` | 结构化日志 |
| `clap` | CLI 参数解析 |
| `globset` | Glob 模式匹配 |
| `serde` / `toml` | 配置序列化 |
| `chrono` | 时间处理 |
| `uuid` | 唯一标识符 |

## 环境要求

- Rust 1.75+
- Linux、macOS 或 Windows

## 许可证

MIT
