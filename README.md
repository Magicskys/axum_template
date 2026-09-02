# Axum Web Backend Template Project

> [中文文档请见 docs/README.zh-CN.md](docs/README.zh-CN.md)

Documentation: [API](docs/api.md) · [Scheduler](docs/scheduler_usage.md)

This project is a Rust web backend template based on [axum](https://github.com/tokio-rs/axum), suitable for quickly building modern web services. It is ready to use and integrates common backend capabilities.

## Features

- 🚀 **High-performance web framework based on axum**
- 🗄️ **Database connection** (powered by sea-orm, supports SQLite/MySQL/Postgres, etc.)
- 📧 **Email sending** (async SMTP support)
- ⏰ **Task scheduling** (supports concurrency, one-time/recurring/persistent tasks)
- 🧪 **Common APIs** (health check, RBAC auth, system config, task CRUD, scheduler management)
- 🧩 Clear structure, easy to extend
- 🦀 100% Rust ecosystem

## Directory Structure

```
axum_template/
├── src/
│   ├── api/         # Routes and APIs
│   ├── migration/   # Versioned database migrations
│   ├── model/       # Database models
│   ├── service/     # Business logic
│   ├── utils/       # Utilities (e.g. mail)
│   ├── scheduler/   # Task scheduler
│   ├── config.rs    # Config loader
│   └── main.rs      # Entry point
├── Cargo.toml       # Dependencies
├── config.ini       # Config file
└── README.md        # Project description
```

## Quick Start

1. **Clone the project**
   ```bash
   git clone
   cd axum_template
   ```
2. **Configure database and mail**
   - Edit `config.ini` and fill in your database and mail server info.
3. **Build and run**
   ```bash
   cargo run
   ```
4. **Access APIs**
   - Health check: `GET /health`
   - App info: `GET /app-info`
   - User auth: `POST /user/register`, `POST /user/login`, `POST /user/logout`, `GET /user/me`
   - System config: `GET /system-config`, `GET/POST/PUT/DELETE /system-config/{key}`
   - Task CRUD: `GET/POST /tasks`, `GET/PUT/DELETE /tasks/{id}`
   - Scheduler tasks: `GET/POST /scheduler/tasks`, `GET/DELETE /scheduler/tasks/{id}`, `POST /scheduler/tasks/{id}/run`

To create an administrator without starting the server:

```bash
cargo run -- create-admin [--username admin] [--password 'your-password']
```

The username defaults to `admin`; omitting the password generates a 20-character alphanumeric password and prints it once in the terminal.

Override the HTTP listener for one run:

```bash
cargo run -- --bind-ip 127.0.0.1 --port 3000
```

## Main Features

### 1. Database Connection
- Uses `sea-orm` as ORM, supports multiple databases.
- Config in `config.ini`, models in `src/model/`.
- Runs versioned SeaORM migrations on startup to create and upgrade users, sessions, tasks, and system config tables.
- For SQLite file URLs, startup enables read/write/create mode, so a missing database file is created automatically.
- RBAC initialization and default system configuration seeding are idempotent and do not overwrite existing values.

### 2. Email Sending
- Uses `lettre`, supports async SMTP mail.
- Config in `config.ini`, usage in `src/utils/mail.rs`.

### 3. Task Scheduling
- Built-in high-performance scheduler, supports:
  - One-time tasks
  - Recurring tasks
  - Scheduled tasks
  - Persistent tasks
  - Concurrency, timeout, retry
- See `src/scheduler/task_scheduler.rs` for details.

### 4. API Response and Error Shape
- New APIs use a unified JSON response:
  ```json
  { "success": true, "message": "ok", "data": {} }
  ```
- API errors return an HTTP status code plus the same response envelope.

### 5. Authentication and Permissions
- Login returns a 24-hour Bearer token. Send it as `Authorization: Bearer <token>`.
- The first registered user receives the `admin` role; later users receive the `user` role.
- Protected handlers declare permissions with typed extractors such as `Required<SystemConfigRead>`.
- Permission markers are centralized in `src/api/auth.rs`, keeping HTTP authorization separate from business services.

## Use Cases
- Rapid development of enterprise/personal web backend services
- Projects needing database, mail, and task scheduling
- As a Rust web project scaffold

## Dependencies
- axum
- sea-orm
- lettre
- tokio
- serde/serde_json
- tracing
- async-trait
- uuid
- chrono

## Contributing
PRs and issues are welcome!
