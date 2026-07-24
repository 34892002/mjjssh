# MJJSSH

[![中文](https://img.shields.io/badge/中文-点击查看-orange)](readme.md)
[![English](https://img.shields.io/badge/English-Click-yellow)](README-en.md)

多窗口 SSH 客户端，基于 Tauri + Vue 3 + Rust 构建。

## 功能特性

- **多会话终端**：以页签管理多个 SSH 连接，支持双击快速连接、终端大小自适应和 WebGL 渲染。
- **连接与安全**：支持密码、SSH 私钥和 SSH 用户证书认证；首次连接时校验并确认主机密钥指纹。
- **主机与密钥管理**：集中保存主机配置和密钥，可生成 Ed25519 或 RSA 4096 位 SSH 密钥，并在连接间复用。
- **SFTP 文件管理**：在会话中浏览远端文件，上传、下载、新建目录、重命名、删除、修改权限，以及打包或解压 `.tar.gz` 文件。
- **AI 助手**：可配置兼容 OpenAI API 的服务，在终端中协助生成或执行操作；高风险操作需要手动确认。
- **加密云同步**：可通过 GitHub Gist 或 Gitee 私有代码片段在设备间同步主机和密钥配置。同步数据使用独立同步密码、Argon2id 和 AES-256-GCM 加密。
- **同步保护**：支持手动或自动同步；发生冲突时可选择保留本地或远端数据，并自动保留冲突备份。
- **服务器概览**：可读取远端操作系统及 IP 地理位置等基本信息。

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面应用 | Tauri 2 + Rust |
| 界面 | Vue 3 + TypeScript + Vite + Naive UI |
| SSH 与终端 | russh + xterm.js + WebGL |
| 本地数据 | JSON Vault |
| 云同步加密 | Argon2id + AES-256-GCM |

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
