# MJJSSH

多窗口 SSH 客户端，基于 Tauri + Vue 3 + Rust 构建。

## 功能特性

- 🔐 加密存储 SSH 凭证（AES-256-GCM + Argon2id）
- 🗂️ 多页签管理，类似 Chrome 浏览器
- 🔑 密钥/证书统一管理，支持复用
- 🌙 深色主题（Catppuccin Mocha）
- ⚡ 快速连接，双击即连

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | Vue 3 + TypeScript + Vite + Naive UI |
| 后端 | Rust + Tauri 2 + russh |
| 数据库 | SQLite + AES-256-GCM 加密 |
| 终端 | xterm.js + WebGL |

## 项目结构

```
my-ssh/
├── docs/
│   └── db.md                  # 数据库设计文档
├── my-ssh-frontend/
│   ├── src/
│   │   ├── App.vue            # 主界面（页签 + 侧栏 + 终端）
│   │   ├── components/        # 组件
│   │   │   ├── Terminal.vue       # xterm.js 终端
│   │   │   ├── ConnectionDialog.vue  # 连接状态弹窗
│   │   │   └── KeysView.vue      # 密钥管理
│   │   ├── stores/
│   │   │   ├── vault.ts      # 凭证库状态
│   │   │   └── session.ts    # SSH 会话状态
│   │   ├── types/             # TypeScript 类型
│   │   └── style.css          # 全局样式
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── vault/         # 加密/数据库
│   │   │   │   ├── crypto.rs      # AES-GCM + Argon2id
│   │   │   │   ├── db.rs          # SQLite 操作
│   │   │   │   └── models.rs      # 数据模型
│   │   │   ├── ssh/           # SSH 连接
│   │   │   │   └── client.rs      # russh 客户端
│   │   │   ├── commands/      # Tauri 命令
│   │   │   └── state.rs       # 应用状态
│   │   └── Cargo.toml
│   └── package.json
└── readme.md
```

## 快速开始

### 环境要求

- Node.js 18+
- Rust 1.77+
- Tauri 系统依赖（Windows 需要 WebView2）

### 安装依赖

```bash
cd my-ssh-frontend
npm install
```

### 开发运行

```bash
npm run dev:tauri
```

### 构建发布

```bash
npm run tauri build
```

## 数据安全

- 所有 SSH 凭证使用 AES-256-GCM 加密存储
- 密钥通过 Argon2id 从主密码派生
- 默认密码 `LuckyMJJ`，可在设置中修改
- 详细设计见 [docs/db.md](docs/db.md)

## 许可证

MIT
