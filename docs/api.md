# API Guide

The service listens on `http://127.0.0.1:8000` by default. Except for registration, login, and health checks, send:

```text
Authorization: Bearer <token>
Content-Type: application/json
```

Responses use `{"success":true,"message":"ok","data":{}}`. Errors use HTTP `400`, `401`, `403`, `404`, or `500` with the same envelope.

## Users and Authentication

- `POST /user/register` (public): accepts `username`, `password` (at least 8 characters), and `email`
- `POST /user/login` (public): returns a 24-hour Bearer token and records the TCP peer IP
- `GET /user/me` (authenticated): returns profile, audit fields, last login IP, and permissions
- `POST /user/logout` (authenticated): invalidates the current token
- `PUT /user/{id}/role` (`user:manage`): replaces the primary role with `{"role":"admin"}`

The first user becomes an administrator. SeaORM RBAC assigns one primary role per user, and the last admin role cannot be removed.

## Tasks

- `GET /tasks`, `GET /tasks/{id}`: `task:read`
- `POST /tasks`, `PUT /tasks/{id}`, `DELETE /tasks/{id}`: `task:write`

Users can access their own tasks; `user:manage` permits cross-user access. Create with `{"action":"send_report","schedule_time":"2026-07-12T10:00:00Z"}`. Administrators may also provide `user_id`.

## Scheduler

- `GET /scheduler/tasks`, `GET /scheduler/tasks/{id}`, `GET /scheduler/tasks/{id}/executions`: `scheduler:read`
- `POST /scheduler/tasks`, `POST /scheduler/tasks/{id}/run`, `DELETE /scheduler/tasks/{id}`: `scheduler:write`

Task types are `one_time`, `recurring`, `scheduled`, and `persistent`. See [Scheduler Usage](scheduler_usage.md) for lifecycle details.

Creation returns HTTP `400` when `executor_type` is not registered. Task definitions and each execution attempt are persisted in `scheduler_tasks` and `task_executions`; the latter records `running`, `completed`, `failed`, or `cancelled` plus finish time and error.

`GET /scheduler/tasks/{id}/executions` returns newest attempts first, for example:

```json
{"success":true,"message":"ok","data":[{"id":"...","task_id":"...","attempt":1,"status":"completed","started_at":"2026-09-06T08:00:00Z","finished_at":"2026-09-06T08:00:02Z","error":null}]}
```

## System Configuration

- `GET /system-config`, `GET /system-config/{key}`: `system_config:read`
- `POST /system-config/{key}`, `PUT /system-config/{key}`, `DELETE /system-config/{key}`: `system_config:write`

Create or update with `{"content":"text or serialized JSON"}`.

## System Status

- `GET /health`: public
- `GET /app-info`: `system_config:read`

Permission markers live in `src/api/auth.rs`. SeaORM RBAC stores them in its `sea_orm_*` tables under the logical `api` resource. Add `Required<PermissionType>` to a handler to perform authentication and authorization before business logic runs.
