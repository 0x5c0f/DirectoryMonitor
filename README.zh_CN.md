# Directory Monitor (Rust)

跨平台文件系统监控工具，使用 Rust 编写。灵感来源于 Windows 平台的 [DirectoryMonitor](https://directorymonitor.com)。

[English](README.md) | 中文

## 功能特性

- **实时监控** — 检测文件创建、修改、删除、重命名、访问等操作
- **详细事件类型** — 对齐 inotifywait 风格：`CREATE`、`MODIFY`、`ATTRIB`、`CLOSE_WRITE`、`OPEN`、`MOVED_FROM`、`MOVED_TO`、`DELETE`、`ACCESS`
- **跨平台** — 支持 Windows、Linux、macOS（基于 `notify` crate）
- **事件过滤** — 按 glob 模式（`include`/`exclude`）和事件类型（`event_types`）筛选
- **事件去重** — 按（路径, 事件类型）去重，保留不同类型的事件
- **批量处理** — 可配置批量大小和超时时间
- **SQLite 存储** — WAL 模式，高吞吐事件持久化
- **通知系统** — 邮件（SMTP）、Syslog（RFC3164/5424）、脚本执行
- **宏系统** — 日志格式和脚本参数支持上下文宏（`%file%`、`%path%`、`%event%` 等）
- **目录快照** — 断网/断电后检测变更
- **CLI 命令行** — 完整命令行工具，支持 run、validate、snapshot 子命令

## 项目结构

```
crates/
├── dm-core/          核心类型：事件、配置、错误定义
├── dm-watcher/       文件系统监控引擎（notify 封装 + 防抖）
├── dm-processor/     事件过滤、去重、批量处理
├── dm-storage/       SQLite 事件持久化
├── dm-notify/        通知系统（邮件、Syslog、脚本）
└── dm-cli/           CLI 入口
```

## 快速开始

```bash
# 构建
make

# 复制并编辑配置文件
cp config.example.toml config.toml
# 编辑 config.toml，配置监控目录

# 验证配置
make validate

# 开始监控
./target/release/directory-monitor -c config.toml run
```

## 安装

```bash
# 安装到 ~/.local/bin
make install

# 卸载
make uninstall
```

## 配置说明

详见 [`config.example.toml`](config.example.toml)，每个配置项都有中文注释。

### 基本配置示例

```toml
[[watches]]
path = "/home/user/documents"
recursive = true
include = ["*.txt", "*.doc", "*.pdf"]
exclude = ["*.tmp", "~*"]
event_types = ["created", "modified", "deleted", "renamed"]
```

### 事件类型

配置文件中的事件类型名称**不区分大小写**，支持多种格式：
- 大写原形：`CREATE`, `MODIFY`, `DELETE`
- 小写原形：`create`, `modify`, `delete`
- 小写过去式：`created`, `modified`, `deleted`

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
| `%event%` | 事件类型 |
| `%path%` | 完整路径 |
| `%target%` | 重命名目标路径 |
| `%timestamp%` | 事件时间戳 |
| `%user%` | 操作用户（PRO 功能） |
| `%process%` | 操作进程（PRO 功能） |

## Make 命令

```bash
make              # 构建 release 版本
make build        # 构建 debug 版本
make test         # 运行测试
make lint         # 代码检查（clippy）
make fmt          # 代码格式化
make clean        # 清理构建产物
make dist         # 打包分发 tarball
make install      # 安装到 ~/.local/bin
make uninstall    # 卸载
make run          # 使用示例配置运行
make validate     # 验证配置文件
make serve        # 启动 Web 服务模式（浏览器访问）
make watch        # 监听文件变更自动重新构建
make help         # 查看所有命令
```

## 运行模式

### 命令行模式（默认）

```bash
./directory-monitor run
```

直接在终端输出事件日志。

### Web 服务模式

```bash
./directory-monitor serve              # 默认 http://127.0.0.1:8080
./directory-monitor serve -b 0.0.0.0:9090  # 自定义地址（局域网访问）
```

启动后在浏览器打开地址，即可查看实时事件日志。支持：
- 实时事件流（WebSocket）
- 历史事件查询
- 事件类型过滤
- 路径搜索

## 架构设计

```
文件系统 → notify → FsWatcher → [防抖 200ms] → WatchEvent
    → broadcast 通道 → 多消费者:
        → EventProcessor (过滤 → 去重 → 批量)
        → EventStore (SQLite)
        → 通知器 (邮件、Syslog、脚本)
        → Web Server (WebSocket 实时推送)
```

事件通过 `tokio::sync::broadcast` 通道分发，支持多个消费者并发处理。

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

## 许可证

MIT
