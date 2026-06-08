# Directory Monitor

跨平台文件系统监控工具，使用 Rust 编写。

[English](https://github.com/0x5c0f/DirectoryMonitor/blob/main/README.md) | 中文

## 快速开始

```bash
# 复制并编辑配置文件
cp config.toml config.toml.bak
# 编辑 config.toml，配置监控目录

# 验证配置
./directory-monitor -c config.toml validate

# 开始监控（命令行模式）
./directory-monitor -c config.toml run

# 启动 Web 仪表盘
./directory-monitor -c config.toml serve
```

## 命令行

```bash
directory-monitor [OPTIONS] [COMMAND]

Options:
  -c, --config <CONFIG>      配置文件路径 [默认: config.toml]
  -l, --log-level <LEVEL>    日志级别 (trace, debug, info, warn, error) [默认: info]
  -h, --help                 显示帮助
  -V, --version              显示版本

Commands:
  run       开始监控（默认）
  serve     启动 Web 仪表盘
  validate  验证配置文件
  snapshot  创建目录快照（用于断网恢复）
```

## Web 仪表盘

```bash
# 默认 http://127.0.0.1:8080
./directory-monitor serve

# 自定义地址
./directory-monitor serve -b 0.0.0.0:9090
```

功能：
- 实时事件流（WebSocket）
- 事件日志分页查询
- 仪表盘图表
- 配置在线编辑
- 可选密码认证

## 配置说明

编辑 `config.toml`，详见文件内注释。

最简配置：

```toml
[[watches]]
path = "/path/to/watch"
recursive = true
event_types = ["created", "modified", "deleted"]
```

## 更多信息

- 完整文档：https://github.com/0x5c0f/DirectoryMonitor
- 问题反馈：https://github.com/0x5c0f/DirectoryMonitor/issues

## 许可证

MIT
