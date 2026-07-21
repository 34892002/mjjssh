# MJJSSH Vault 存储设计

> 本文描述当前 JSON Vault 架构。旧 SQLite、`local.key` 和字段级加密格式不提供迁移、导入或兼容路径；完整同步流程见 [cloud-sync.md](cloud-sync.md)。

## 1. 存储策略

Vault 的唯一业务数据格式为 JSON，存储位置：`<程序目录>/data/vault.json`。

- 未启用云同步时，本地 Vault 为明文 JSON，首次启动和日常使用均不要求密码。
- 不使用固定默认密码，不生成或使用 `local.key`，不依赖特定平台的系统密钥库。
- 启用云同步后，上传至 GitHub Gist 或 Gitee 私有片段的副本使用同步密码整体加密；本地 `vault.json` 仍保持明文。
- 本地明文 Vault 的安全边界是操作系统账户权限、程序目录权限和磁盘加密。产品界面和文档必须明确此取舍。

本地与云端只有一个业务数据模型，不使用 SQLite 与 JSON 双写，也不使用 JSONL。

## 2. 本地文件格式

```json
{
  "formatVersion": 1,
  "vaultId": "b9b92c0e-0f4d-4b64-8f1a-53f7d4f56b9e",
  "revision": 18,
  "updatedAt": "2026-07-20T12:00:00Z",
  "profiles": [],
  "sshKeys": [],
  "aiProviderConfig": null,
  "aiAgents": [],
  "aiExecutableGrants": []
}
```

- `formatVersion` 用于文件格式迁移。
- `vaultId` 是创建 Vault 时生成且不变的 UUID。
- `revision` 每次成功的本地写入递增；同步仅将其作为辅助信息，不能以设备时间决定覆盖顺序。
- `updatedAt` 使用 RFC3339，仅用于展示和诊断。

每次变更都先修改内存模型、执行完整性校验，再写入同目录临时文件并通过原子重命名替换 `vault.json`。替换前保留 `vault.json.bak`，以便从写入中断或文件损坏中恢复。

## 3. 业务数据模型

### 3.1 `profiles`（SSH 主机配置）

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | string | 是 | UUID |
| `name` | string | 是 | 连接名称 |
| `host` | string | 是 | 主机地址 |
| `port` | number | 是 | SSH 端口，默认 `22` |
| `username` | string | 是 | 登录用户名 |
| `authType` | string | 是 | `password` / `key` / `certificate` |
| `credential` | string / null | 否 | 密码认证的密码；本地明文，仅在云端整体加密 |
| `keyId` | string / null | 否 | 引用 `sshKeys[].id` |
| `groupName` | string / null | 否 | 分组名称 |
| `icon` | string / null | 否 | 图标标识 |
| `color` | string / null | 否 | 图标颜色 |
| `os` | string / null | 否 | 操作系统信息 |
| `location` | string / null | 否 | 地区或位置 |
| `createdAt` | string | 是 | RFC3339 |
| `updatedAt` | string | 是 | RFC3339 |

### 3.2 `sshKeys`（SSH 私钥和证书）

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | string | 是 | UUID |
| `name` | string | 是 | 密钥名称 |
| `keyType` | string | 是 | `key` / `certificate` |
| `privateKey` | string | 是 | 私钥内容；本地明文，仅在云端整体加密 |
| `certData` | string / null | 否 | SSH 用户证书内容；本地明文，仅在云端整体加密 |
| `createdAt` | string | 是 | RFC3339 |
| `updatedAt` | string | 是 | RFC3339 |

### 3.3 AI 配置与授权

- `aiProviderConfig`：单个服务地址、模型、超时和 API Key 配置，可为空。
- `aiAgents`：Agent 名称、提示词和默认 Agent 标记。
- `aiExecutableGrants`：确认模式中用户授予的可执行程序权限。

API Key 和 Agent 提示词在本地 JSON 中为明文；同步时和全部 Vault 数据一起加密。AI 操作审计记录、完整终端输出、完整 AI 响应以及应用日志不进入 Vault。

## 4. 关联与校验

```text
profiles[].keyId -> sshKeys[].id
```

- `authType: "certificate"` 必须引用 `keyType: "certificate"` 且有 `certData` 的 SSH 密钥。
- 写入、导入和云端下载解密后都必须校验 UUID 唯一性和所有 `keyId` 引用。
- 删除密钥时，必须清除引用它的 profile `keyId`，或在 UI 中拒绝删除；实现须选择并保持一致的行为。
- 新格式不在 profile 中重复存储 `privateKey` 或 `certData`。迁移旧数据时，旧字段需转换到 `sshKeys`。

## 5. 云端同步加密

未配置同步密码时不进行加密，也不创建云端副本。

启用云同步后，应用读取本地完整 Vault JSON，使用用户输入的同步密码生成远端密文文件。同步密码是唯一用户密码：它不影响本地使用，不是 GitHub/Gitee 登录密码，也不会上传或持久化。

```text
本地 vault.json（明文业务 JSON）
        |
        | 同步密码 + 随机 salt -> Argon2id
        v
AES-256-GCM 密钥
        |
        | 加密完整业务 JSON
        v
远端 mjjssh-vault.json（整体密文）
```

远端封装示例：

```json
{
  "formatVersion": 1,
  "vaultId": "b9b92c0e-0f4d-4b64-8f1a-53f7d4f56b9e",
  "revision": 18,
  "updatedAt": "2026-07-20T12:00:00Z",
  "updatedByDeviceId": "ee1cffb9-2f55-479d-8f84-a6f4a33f7c33",
  "encryption": {
    "kdf": "argon2id",
    "kdfVersion": 1,
    "memoryKiB": 65536,
    "iterations": 3,
    "parallelism": 4,
    "salt": "base64...",
    "cipher": "aes-256-gcm",
    "nonce": "base64..."
  },
  "ciphertext": "base64..."
}
```

使用 AES-GCM AAD 认证 `formatVersion`、`vaultId`、`revision` 和规范化 KDF 参数。任何密文、nonce、版本或 KDF 参数篡改都必须使解密失败。远端只保存此单个文件，不能按字段或对象拆分成多个片段。

错误密码与损坏文件在 UI 中统一显示为“同步密码错误或同步数据已损坏”，避免泄露验证细节。同步密码最少 8 个字符，允许空格和长密码短语且不设置上限；忘记后无法恢复旧云端数据。

## 6. 旧文件处理

`<程序目录>/data/local.key` 与 `<程序目录>/data/vault.db` 属于已移除的 SQLite/字段级加密架构。当前程序不会读取、修改、迁移或导入这些文件。确认不再需要旧数据后，可由用户自行删除；删除前应自行备份，因为其中的数据不会自动转换到 `vault.json`。

完整的同步流程、冲突策略、远端 API 边界和验收清单见 [cloud-sync.md](cloud-sync.md)。