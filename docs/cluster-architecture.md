# 集群架构设计文档

## 概述

Directory Monitor 支持多节点集群部署，通过 gRPC 直连实现节点间通信（事件同步、心跳、查询聚合）。每个节点独立监控本地文件系统，集群层面实现事件聚合和节点发现。

**优势**：零外部依赖，无需部署消息中间件（如 NATS），简化运维。

## 架构图

```
┌─────────────────┐                ┌─────────────────┐
│   Node A        │◄──gRPC 直连──►│   Node B        │
│                 │  (事件/心跳)    │                 │
│ ┌─────────────┐ │               │ ┌─────────────┐ │
│ │ Local Watch │ │               │ │ Local Watch │ │
│ │   + SQLite  │ │               │ │   + SQLite  │ │
│ └─────────────┘ │               │ └─────────────┘ │
│ ┌─────────────┐ │               │ ┌─────────────┐ │
│ │ gRPC Server │ │               │ │ gRPC Server │ │
│ │ (端口 9101) │ │               │ │ (端口 9101) │ │
│ └─────────────┘ │               │ └─────────────┘ │
│ ┌─────────────┐ │               │ ┌─────────────┐ │
│ │PeerManager  │ │               │ │PeerManager  │ │
│ │(连接管理)   │ │               │ │(连接管理)   │ │
│ └─────────────┘ │               │ └─────────────┘ │
└────────┬────────┘               └────────┬────────┘
         │                                 │
         │         gRPC 查询               │
         └────────────────┬────────────────┘
                          │
              ┌───────────▼───────────┐
              │ ClusterQueryAggregator │
              │ (聚合本地+远程结果)     │
              └───────────────────────┘
```

## 核心组件

### 1. PeerManager（连接管理器）

**职责**：
- 管理到所有 peer 节点的 gRPC 连接
- 提供 fan-out 事件发布（`publish_event_to_all`）
- 提供 fan-out 心跳发送（`send_heartbeat_to_all`）
- 后台自动重连失败的 peer

**配置**：
```toml
[[cluster.peers]]
addr = "10.0.0.2:9101"
```

### 2. EventSyncService（事件同步服务）

**职责**：
- 发布本地事件到所有 peer（通过 PeerManager fan-out）
- 接收远程事件并存入 EventCache

**数据流**：
```
本地 WatchEvent → EventSyncService → PeerManager → 所有 peer 的 PublishEvent RPC
                              ↓
                        EventCache (ring buffer)
```

### 3. HeartbeatService（心跳服务）

**职责**：
- 定期发送自身心跳到所有 peer（包含 watcher_count, event_count）
- 接收远程心跳并更新 NodeRegistry
- 检测超时节点并标记为 Offline

### 4. NodeRegistry（节点注册表）

**职责**：
- 维护集群节点列表（本地 + 远程）
- 提供节点状态查询（Online/Offline/Unknown）
- 供 ClusterQueryAggregator 使用

### 5. gRPC Server

**职责**：
- 提供远程事件查询接口（`QueryEvents` RPC）
- 提供节点状态查询接口（`GetNodeStatus` RPC）
- 接收远程事件（`PublishEvent` RPC）
- 接收远程心跳（`Heartbeat` RPC）

**默认端口**：9101（避免与 node_exporter 的 9100 冲突）

### 6. ClusterQueryAggregator（查询聚合器）

**职责**：
- 聚合本地 + 远程节点的查询结果
- 支持按节点过滤（node_id 参数）
- 结果去重、排序、分页

**查询流程**：
```
1. 查询本地 SQLite
2. 查询远程节点（通过 gRPC）
3. 查询 EventCache（最近的远程事件）
4. 按时间戳降序排序
5. 按事件 ID 去重
6. 应用 limit
```

## gRPC 服务定义

```protobuf
service ClusterService {
    // 查询本节点事件
    rpc QueryEvents (QueryEventsRequest) returns (QueryEventsResponse);

    // 获取本节点状态
    rpc GetNodeStatus (NodeStatusRequest) returns (NodeStatusResponse);

    // 发布事件到本节点（fan-out 从源节点调用所有 peer）
    rpc PublishEvent (PublishEventRequest) returns (PublishEventResponse);

    // 发送心跳到本节点
    rpc Heartbeat (HeartbeatRequest) returns (HeartbeatResponse);
}
```

## Web API

### GET /api/events

**集群模式参数**：
- `node_id` — 按节点 ID 过滤（可选）
- 其他参数与单机模式相同

**响应**：
```json
{
  "events": [
    {
      "id": "uuid",
      "timestamp": "2026-06-09T03:30:00Z",
      "event_type": "CREATE",
      "path": "/path/to/file",
      "target_path": null,
      "is_dir": false,
      "watch_root": "/watch",
      "node_id": "node-a-uuid",
      "node_name": "node-a"
    }
  ],
  "total": 100,
  "page": 1,
  "per_page": 50,
  "total_pages": 2
}
```

### GET /api/cluster/status

返回集群状态信息。

### GET /api/cluster/nodes

返回所有已知节点列表（包括通过心跳发现的远程节点）。

## 配置示例

```toml
[cluster]
enabled = true
node_name = "production-server-1"
# node_id 留空则自动生成 UUID
listen_addr = "0.0.0.0:9101"
heartbeat_interval_secs = 5
node_timeout_secs = 30
event_cache_size = 10000

[[cluster.peers]]
addr = "10.0.0.2:9101"
```

## 部署步骤

1. 在每个节点的 `config.toml` 中启用集群：
   ```toml
   [cluster]
   enabled = true
   node_name = "node-1"
   listen_addr = "0.0.0.0:9101"
   
   [[cluster.peers]]
   addr = "其他节点IP:9101"
   ```

2. 确保防火墙开放 9101 端口

3. 启动每个节点的 directory-monitor 服务

## 安全建议

1. **防火墙**：限制 9101 端口的访问源 IP
2. **TLS**：生产环境建议启用 gRPC TLS（配置 `cluster.tls`）
3. **网络**：确保节点间网络互通

## 设计原则

1. **本地优先**：每个节点独立监控和存储，不依赖集群
2. **聚合查询**：集群查询是叠加能力，降级不影响单机功能
3. **事件去重**：通过事件 UUID 确保跨节点事件不重复
4. **最终一致**：通过心跳实现节点发现，允许短暂的状态不一致
5. **零依赖**：不需要外部消息中间件，简化部署和运维
