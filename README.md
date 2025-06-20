# Axum Web Backend Template Project

> [中文文档请见 docs/README.zh-CN.md](docs/README.zh-CN.md)

This project is a Rust web backend template based on [axum](https://github.com/tokio-rs/axum), suitable for quickly building modern web services. It is ready to use and integrates common backend capabilities.

## Features

- 🚀 **High-performance web framework based on axum**
- 🗄️ **Database connection** (powered by sea-orm, supports SQLite/MySQL/Postgres, etc.)
- 📧 **Email sending** (async SMTP support)
- ⏰ **Task scheduling** (supports concurrency, one-time/recurring/persistent tasks)
- 🧩 Clear structure, easy to extend
- 🦀 100% Rust ecosystem

## Directory Structure

```
axum_template/
├── src/
│   ├── api/         # Routes and APIs
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
   - See implementations in `src/api/`.

## Main Features

### 1. Database Connection
- Uses `sea-orm` as ORM, supports multiple databases.
- Config in `config.ini`, models in `src/model/`.

### 2. Email Sending
- Uses `lettre`, supports async SMTP mail.
- Config in `config.ini`, usage in `src/utils/mail.rs`.

### 3. Task Scheduling
- Built-in high-performance scheduler, supports:
  - One-time tasks
  - Recurring tasks
  - Persistent tasks
  - Concurrency, timeout, retry
- See `src/scheduler/task_scheduler.rs` for details.

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