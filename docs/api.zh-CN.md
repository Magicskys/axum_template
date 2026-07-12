# API 文档

服务默认监听 `http://127.0.0.1:8000`。除注册、登录和健康检查外，接口均使用：

```text
Authorization: Bearer <token>
Content-Type: application/json
```

统一响应结构：

```json
{"success":true,"message":"ok","data":{}}
```

错误使用对应 HTTP 状态码：`400` 参数错误、`401` 未登录或会话失效、`403` 权限不足、`404` 资源不存在、`500` 服务错误。

## 用户与认证

### `POST /user/register`（公开）

```json
{"username":"admin","password":"password123","email":"admin@example.com"}
```

密码至少 8 位。首个用户获得 `admin`，后续用户获得 `user`。用户保存邮箱、启用状态、创建/更新时间及最后登录时间和 IP。

### `POST /user/login`（公开）

```json
{"username":"admin","password":"password123"}
```

返回 `token`、`token_type`、`expires_at`、`user_id`、`last_login_at`。Token 有效期 24 小时。登录 IP 来自 TCP 对端地址，不信任客户端代理头。

### `GET /user/me`（登录）

返回当前用户的 `id`、`username`、`email`、`is_active`、审计时间、`last_login_ip` 和权限列表。

### `POST /user/logout`（登录）

删除当前会话，原 token 立即失效。

### `PUT /user/{id}/roles`（`user:manage`）

```json
{"roles":["admin","user"]}
```

替换用户角色。系统拒绝移除最后一个管理员。

## 普通任务

普通用户只能操作自己的任务；具有 `user:manage` 的用户可跨用户访问。

- `GET /tasks`：`task:read`，管理员可用 `?user_id=1` 筛选
- `GET /tasks/{id}`：`task:read`
- `POST /tasks`：`task:write`
- `PUT /tasks/{id}`：`task:write`
- `DELETE /tasks/{id}`：`task:write`

创建：

```json
{"action":"send_report","schedule_time":"2026-07-12T10:00:00Z"}
```

管理员可额外传 `user_id`。更新时 `action`、`schedule_time` 均为可选字段。

## 调度器

- `GET /scheduler/tasks`：`scheduler:read`
- `GET /scheduler/tasks/{id}`：`scheduler:read`
- `POST /scheduler/tasks`：`scheduler:write`
- `POST /scheduler/tasks/{id}/run`：`scheduler:write`
- `DELETE /scheduler/tasks/{id}`：`scheduler:write`

创建循环任务：

```json
{
  "name":"cleanup",
  "executor_type":"cleanup_executor",
  "task_type":"recurring",
  "interval_seconds":60,
  "timeout_seconds":30,
  "max_retries":3,
  "data":{"scope":"expired"}
}
```

`task_type` 可为 `one_time`、`recurring`、`scheduled`、`persistent`。Scheduled 使用 `next_run`，Persistent executor 应持续运行直到任务被删除或服务停止。

## 系统配置

- `GET /system-config`：`system_config:read`
- `GET /system-config/{key}`：`system_config:read`
- `POST /system-config/{key}`：`system_config:write`，创建
- `PUT /system-config/{key}`：`system_config:write`，更新
- `DELETE /system-config/{key}`：`system_config:write`

创建或更新请求：

```json
{"content":"配置内容，可以是 JSON 字符串或普通文本"}
```

## 系统状态

- `GET /health`：公开
- `GET /app-info`：`system_config:read`

## 权限扩展

权限码和类型标记位于 `src/api/auth.rs`。新增受保护接口时定义标记，并把 `Required<权限类型>` 放入 handler 参数即可同时完成认证和授权。
