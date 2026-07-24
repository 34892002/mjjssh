# MJJSSH

[![中文](https://img.shields.io/badge/中文-点击查看-yellow)](readme.md)
[![English](https://img.shields.io/badge/English-Click-yellow)](README-en.md)

A multi-window SSH client built with Tauri, Vue 3, and Rust.

## Features

- **Multi-session terminal**: Manage multiple SSH connections in tabs, with double-click quick connect, responsive terminal resizing, and WebGL rendering.
- **Connection and security**: Supports password, SSH private-key, and SSH user-certificate authentication. Host-key fingerprints are verified and confirmed on first connection.
- **Host and key management**: Centrally store host profiles and keys, generate Ed25519 or RSA 4096-bit SSH keys, and reuse them across connections.
- **SFTP file management**: Browse remote files within a session; upload, download, create directories, rename, delete, change permissions, and create or extract `.tar.gz` archives.
- **AI assistant**: Configure an OpenAI-compatible API service to assist with terminal operations. High-risk actions require manual confirmation.
- **Encrypted cloud sync**: Sync host and key configurations between devices through GitHub Gist or Gitee private snippets. Sync data is encrypted with a separate sync password, Argon2id, and AES-256-GCM.
- **Sync protection**: Supports manual or automatic sync. When conflicts occur, choose the local or remote version; conflict backups are retained automatically.
- **Server overview**: Read basic remote-server information such as the operating system and IP geolocation.

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

## Data Storage and Cloud Sync

- The local Vault is stored at `<application directory>/data/vault.json`. When cloud sync is disabled, no password is required and the file is stored unencrypted. OS account security, disk encryption, and file permissions are the protection boundary for local data.
- When the user enables cloud sync, the complete Vault is encrypted with a key derived from the sync password using Argon2id and AES-256-GCM before being uploaded to GitHub Gist or a Gitee private snippet.
- The sync password does not alter local SSH credentials, is never uploaded, and cannot be recovered. The initial release supports manual upload/download only and does not automatically merge changes from multiple devices.
- When a conflict occurs, choose to keep the local or remote version. Before overwriting, the application backs up the local Vault and the remote encrypted envelope to `data/sync-conflicts/`.
- See [docs/cloud-sync.md](docs/cloud-sync.md) for the design, known limitations, and verification notes.

## License

MIT
