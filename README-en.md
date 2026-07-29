# MJJSSH

[![中文](https://img.shields.io/badge/中文-点击查看-orange)](readme.md)
[![English](https://img.shields.io/badge/English-Click-yellow)](README-en.md)

A lightweight, multi-window SSH client built with Tauri 2, Vue 3, and Rust. Release installers are approximately 6 MB and provide terminal access, file management, AI assistance, encrypted cloud sync, and script subscriptions.

[Download the latest release](https://github.com/34892002/mjjssh/releases) | [View the source code](https://github.com/34892002/mjjssh)

## Preview

![Main window](https://cdn.nodeimage.com/i/A2EZ4DiaNLWJ9urotxrW4HN8XtLgXtQi.webp)

![SSH terminal](https://cdn.nodeimage.com/i/z7fnfpVyIDrkUPZedoq7rwYFXfMWVmzD.webp)

## Features

- **Multi-session terminal**: Manage multiple SSH connections in tabs, with double-click quick connect, responsive terminal resizing, and WebGL rendering.
- **Connection and security**: Supports password, SSH private-key, and SSH user-certificate authentication. Host-key fingerprints are verified and confirmed on first connection.
- **Host and key management**: Centrally store host profiles and keys, generate Ed25519 or RSA 4096-bit SSH keys, and reuse them across connections.
- **SFTP file management**: Browse remote files within a session; upload, download, create directories, rename, delete, change permissions, and create or extract `.tar.gz` archives.
- **AI assistant**: Configure an OpenAI-compatible API service with Q&A, manual-execution, and automatic-execution modes. High-risk actions require manual confirmation.
- **Encrypted cloud sync**: Sync host and key configurations between devices through GitHub Gist or Gitee private snippets. Cloud data is encrypted with a separate sync password, Argon2id, and AES-256-GCM.
- **Sync protection**: Supports manual or automatic sync. When conflicts occur, choose the local or remote version; conflict backups are retained automatically.
- **Script subscriptions**: Maintain and subscribe to collections of common operations scripts, with risk warnings before high-risk scripts are run.
- **Proxy support**: Configure a proxy for SSH connections.
- **Server overview**: Read basic remote-server information such as the operating system and IP geolocation.

## Screenshots

### AI Assistant

![AI assistant](https://cdn.nodeimage.com/i/Ie7cHbnQuA7N8fGEKYjOLGhE5uS995b1.webp)

### Cloud Sync

![Cloud sync](https://cdn.nodeimage.com/i/UVFTT6rMZuMLfv7QwkecIbYSjzYo1j9p.webp)

### Script Subscriptions

![Script subscription list](https://cdn.nodeimage.com/i/WGb9KMP212wDs7fuRCNd1Og1GhFApNph.webp)

![Script subscription details](https://cdn.nodeimage.com/i/t0R619vpuT2Vo4t9ayR9Rq10x0SR4MSc.webp)

### File Management and Proxy

![File management](https://cdn.nodeimage.com/i/hFJnERc679j5IEKFSYjJRQYZ15fviDvr.webp)

![Proxy settings](https://cdn.nodeimage.com/i/xAit5GTnXDR8jzarBwACwQoXYk64KxdB.webp)

## Technology

| Layer | Technology |
|------|------|
| Desktop application | Tauri 2 + Rust |
| Interface | Vue 3 + TypeScript + Vite + Naive UI |
| SSH and terminal | russh + xterm.js + WebGL |
| Local data | JSON Vault |
| Cloud sync encryption | Argon2id + AES-256-GCM |

## Quick Start

### Requirements

- Node.js 18+
- Rust 1.77+
- Tauri system dependencies (WebView2 on Windows)

### Install dependencies

```bash
cd my-ssh-frontend
npm install
```

### Run in development

```bash
npm run dev:tauri
```

### Build a release

```bash
npm run tauri build
```

## Data Security

- **Local data encryption**: The local Vault is stored at `<application directory>/data/vault.json` and encrypted with a randomly generated data key and AES-256-GCM. The operating system credential manager stores the key, so normal use does not require a local password.
- **Separate cloud password**: The cloud copy never reuses the local data key. When sync is enabled, a separate sync password is processed with Argon2id to derive a key that encrypts the complete Vault with AES-256-GCM before upload to GitHub Gist or a Gitee private snippet.
- **Password scope**: The sync password is never uploaded, is not written to local configuration, and does not alter local SSH credentials. Enter it when configuring sync on a device, importing existing cloud data, or changing the password; the credential manager retains the credentials needed for subsequent syncs.
- **Recovery and conflicts**: The sync password cannot be recovered. When a conflict occurs, choose the local or remote version; before overwriting, the application backs up the local Vault and remote encrypted envelope to `data/sync-conflicts/`.
- See [docs/db.md](docs/db.md) for the storage format and synchronization design.

## License

MIT
