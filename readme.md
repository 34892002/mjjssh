# MJJSSH

[![中文](https://img.shields.io/badge/中文-点击查看-orange)](readme.md)
[![English](https://img.shields.io/badge/English-Click-yellow)](README-en.md)

基于 Tauri 2、Vue 3 和 Rust 构建的轻量级多窗口 SSH 客户端。发布版安装包约 6 MB，提供终端连接、文件管理、AI 辅助、加密云同步和脚本订阅等功能。

[下载最新版本](https://github.com/34892002/mjjssh/releases) | [查看源代码](https://github.com/34892002/mjjssh)

## 界面预览

![主界面](https://cdn.nodeimage.com/i/A2EZ4DiaNLWJ9urotxrW4HN8XtLgXtQi.webp)

![SSH 终端](https://cdn.nodeimage.com/i/z7fnfpVyIDrkUPZedoq7rwYFXfMWVmzD.webp)

## 功能特性

- **多会话终端**：以页签管理多个 SSH 连接，支持双击快速连接、终端大小自适应和 WebGL 渲染。
- **连接与安全**：支持密码、SSH 私钥和 SSH 用户证书认证；首次连接时校验并确认主机密钥指纹。
- **主机与密钥管理**：集中保存主机配置和密钥，可生成 Ed25519 或 RSA 4096 位 SSH 密钥，并在连接间复用。
- **SFTP 文件管理**：在会话中浏览远端文件，上传、下载、新建目录、重命名、删除、修改权限，以及打包或解压 `.tar.gz` 文件。
- **AI 助手**：可配置兼容 OpenAI API 的服务，提供问答、手动执行和自动执行三种模式；高风险操作需要手动确认。
- **加密云同步**：可通过 GitHub Gist 或 Gitee 私有代码片段在设备间同步主机和密钥配置。云端数据使用独立同步密码、Argon2id 和 AES-256-GCM 加密。
- **同步保护**：支持手动或自动同步；发生冲突时可选择保留本地或远端数据，并自动保留冲突备份。
- **脚本订阅**：支持维护和订阅常用运维脚本列表，并在执行高风险脚本前显示风险提示。
- **网络代理**：支持为 SSH 连接配置代理。
- **服务器概览**：可读取远端操作系统及 IP 地理位置等基本信息。

## 功能截图

### AI 助手

![AI 助手](https://cdn.nodeimage.com/i/Ie7cHbnQuA7N8fGEKYjOLGhE5uS995b1.webp)

### 云同步

![云同步](https://cdn.nodeimage.com/i/UVFTT6rMZuMLfv7QwkecIbYSjzYo1j9p.webp)

### 脚本订阅

![脚本订阅列表](https://cdn.nodeimage.com/i/WGb9KMP212wDs7fuRCNd1Og1GhFApNph.webp)

![脚本订阅详情](https://cdn.nodeimage.com/i/t0R619vpuT2Vo4t9ayR9Rq10x0SR4MSc.webp)

### 文件管理与代理

![文件管理](https://cdn.nodeimage.com/i/hFJnERc679j5IEKFSYjJRQYZ15fviDvr.webp)

![代理设置](https://cdn.nodeimage.com/i/xAit5GTnXDR8jzarBwACwQoXYk64KxdB.webp)

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

## 数据安全

- **本地数据加密**：本地 Vault 位于 `<程序目录>/data/vault.json`，使用随机生成的数据密钥和 AES-256-GCM 加密。密钥由操作系统凭据管理器保管，日常使用无需输入本地密码。
- **独立的云端密码**：云端副本不会复用本地数据密钥。启用同步时，使用单独的同步密码经 Argon2id 派生密钥，再以 AES-256-GCM 加密完整 Vault 后上传至 GitHub Gist 或 Gitee 私有代码片段。
- **密码使用范围**：同步密码不会上传、不会写入本地配置，也不影响本地 SSH 凭据。每台设备首次配置同步、导入已有云端数据或更换同步密码时需要输入；之后由系统凭据管理器保存同步所需凭据。
- **数据恢复与冲突**：同步密码无法找回。发生冲突时可保留本地或采用远端数据；覆盖前会备份本地 Vault 与远端加密封装到 `data/sync-conflicts/`。
- 详细的存储格式与同步设计见 [docs/db.md](docs/db.md)。

## 许可证

MIT
