# MJJSSH

多窗口 SSH 客户端，基于 Tauri + Vue 3 + Rust 构建。

## 功能特性

- ☁️ 可选端到端加密云同步（GitHub Gist / Gitee 私有片段）
- 🗂️ 多页签管理，类似 Chrome 浏览器
- 🔑 密钥/证书统一管理，支持复用
- 🌙 深色主题（Catppuccin Mocha）
- ⚡ 快速连接，双击即连

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | Vue 3 + TypeScript + Vite + Naive UI |
| 后端 | Rust + Tauri 2 + russh |
| 持久化 | JSON Vault |
| 云同步加密 | Argon2id + AES-256-GCM |
| 终端 | xterm.js + WebGL |

## 项目结构

```
my-ssh/
├── docs/
│   ├── db.md                  # Vault 存储设计
│   └── cloud-sync.md          # 云同步设计与迁移方案
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
│   │   │   ├── vault/         # 本地 JSON Vault
│   │   │   │   ├── store.rs       # 校验、原子写入与备份恢复
│   │   │   │   └── models.rs      # 数据模型
│   │   │   ├── sync/          # 云端加密与远端适配器
│   │   │   │   ├── crypto.rs      # Argon2id 与 AES-256-GCM
│   │   │   │   ├── github_gist.rs
│   │   │   │   └── gitee_snippet.rs
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

## 数据存储与云同步

- 本地 Vault 位于 `<程序目录>/data/vault.json`；未启用云同步时不要求密码，文件为明文。操作系统账户、磁盘加密和文件权限是本地数据保护边界。
- 用户主动开启云同步后，完整 Vault 使用同步密码经 Argon2id 派生密钥，并以 AES-256-GCM 整体加密后上传至 GitHub Gist 或 Gitee 私有代码片段。
- 同步密码不影响本地 SSH 凭证，不会上传，且无法找回。首版仅支持手动上传/下载，不自动合并多设备修改。
- 冲突处理可保留本地或采用远端；覆盖前会备份本地 Vault 与远端加密封装到 `data/sync-conflicts/`。
- 设计、已知限制和验证记录见 [docs/cloud-sync.md](docs/cloud-sync.md)。

## 许可证

MIT
