# Axum Web 后端模板项目

本项目是一个基于 [axum](https://github.com/tokio-rs/axum) 的 Rust Web 后端模板，适合快速搭建现代 Web 服务。开箱即用，集成了常用的后端能力。

## 特性

- 🚀 **基于 axum 的高性能 Web 框架**
- 🗄️ **数据库连接**（基于 sea-orm，支持 SQLite/MySQL/Postgres 等）
- 📧 **邮件发送**（支持 SMTP，异步发送）
- ⏰ **定时任务调度**（支持并发、一次性/循环/持久化任务）
- 🧩 结构清晰，易于扩展
- 🦀 100% Rust 生态

## 目录结构

```
axum_template/
├── src/
│   ├── api/         # 路由与接口
│   ├── model/       # 数据库模型
│   ├── service/     # 业务逻辑
│   ├── utils/       # 工具函数（如邮件）
│   ├── scheduler/   # 定时任务调度
│   ├── config.rs    # 配置加载
│   └── main.rs      # 启动入口
├── Cargo.toml       # 依赖配置
├── config.ini       # 配置文件
└── README.md        # 项目说明
```

## 快速开始

1. **克隆项目**
   ```bash
   git clone
   cd axum_template
   ```
2. **配置数据库和邮件**
   - 编辑 `config.ini`，填写数据库连接和邮件服务器信息。
3. **编译并运行**
   ```bash
   cargo run
   ```
4. **访问接口**
   - 参考 `src/api/` 目录下的接口实现。

## 主要功能说明

### 1. 数据库连接
- 使用 `sea-orm` 作为 ORM 框架，支持多种数据库。
- 配置见 `config.ini`，模型见 `src/model/`。

### 2. 邮件发送
- 使用 `lettre`，支持异步 SMTP 邮件发送。
- 配置见 `config.ini`，调用见 `src/utils/mail.rs`。

### 3. 定时任务调度
- 内置高性能定时任务调度器，支持：
  - 一次性任务
  - 循环任务
  - 持久化任务
  - 并发执行、超时控制、重试机制
- 详见 `src/scheduler/task_scheduler.rs`

## 适用场景
- 快速开发企业级/个人 Web 后端服务
- 需要数据库、邮件、定时任务等常用能力的项目
- 作为 Rust Web 项目的脚手架

## 依赖
- axum
- sea-orm
- lettre
- tokio
- serde/serde_json
- tracing
- async-trait
- uuid
- chrono

## 贡献
欢迎 issue 和 PR！