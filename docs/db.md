# MJJSSH Vault 存储设计

> 本文描述当前 JSON Vault 架构。业务 JSON 位于本地 AES-256-GCM 加密信封内，数据密钥与云同步凭据由系统凭据管理器保管；完整安全与同步设计见 [config-security-sync.md](config-security-sync.md)。

## 1. 存储策略

Vault 的唯一业务数据格式为 JSON，存储位置：`<程序目录>/data/vault.json`。

- 本地 `vault.json` 和 `vault.json.bak` 均是 AES-256-GCM 加密信封；首次创建时生成随机 32 字节数据密钥。
- 本地数据密钥保存于 Windows Credential Manager 或 macOS Keychain，服务名为 `com.mjjssh.app`；不创建可复制的 `local.key` 文件。
- 启用云同步后，上传至 GitHub Gist 或 Gitee 私有片段的副本继续使用同步密码整体加密。同步 Token 和派生同步密钥只保存于系统凭据管理器，不写入 `sync.json`。
- 操作系统凭据库可减少离线文件复制、备份泄露和其他用户读取的风险，但不能阻止以当前登录用户身份运行的恶意程序。

本地与云端只有一个业务数据模型，不使用 SQLite 与 JSON 双写，也不使用 JSONL。

## 2. 本地文件格式

```json
{
  "formatVersion": 1,
  "cipher": "aes-256-gcm",
  "nonce": "base64...",
  "ciphertext": "base64..."
}
```

信封明文为当前 `formatVersion: 2` 的业务 JSON，包含 `vaultId`、`revision`、SSH 凭据、私钥和 AI API Key。nonce 为每次写入生成的随机 12 字节值，认证数据固定为本地格式标识。

- 信封 `formatVersion` 为本地加密封装版本，当前为 `1`。
- 信封内业务 JSON 的 `vaultId` 是创建 Vault 时生成且不变的 UUID。
- `revision` 每次成功的本地写入递增；同步仅将其作为辅助信息，不能以设备时间决定覆盖顺序。
- `updatedAt` 使用 RFC3339，仅用于展示和诊断。
- 新项目不提供明文 Vault、旧业务格式或旧 `sync.json` 的读取与迁移兼容；发现它们应报错而非降级读取。

每次变更都先修改内存模型、执行完整性校验，再写入同目录加密临时文件并通过原子重命名替换 `vault.json`。替换前保留加密的 `vault.json.bak`，以便从写入中断或文件损坏中恢复。

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
| `credential` | string / null | 否 | 密码认证的密码；包含在本地与云端整体加密的 Vault 中 |
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
| `privateKey` | string | 是 | 私钥内容；包含在本地与云端整体加密的 Vault 中 |
| `certData` | string / null | 否 | SSH 用户证书内容；包含在本地与云端整体加密的 Vault 中 |
| `createdAt` | string | 是 | RFC3339 |
| `updatedAt` | string | 是 | RFC3339 |

### 3.3 用户脚本

`scripts` 保存用户创建的可复用 SSH 命令。每项包含 UUID、唯一名称、可选描述、最多 10 个标签、最大 32 KiB 的原始命令、风险提示级别和创建/修改时间。脚本不保存主机、用户名、凭据、私钥、Token 或环境变量；执行时由用户选择已连接会话。它与其他 Vault 业务数据一起同步时整体加密。

### 3.4 AI 配置与授权

- `aiProviderConfig`：单个服务地址、模型、超时和 API Key 配置，可为空。
- `aiAgents`：Agent 名称、提示词和默认 Agent 标记。
- `aiExecutableGrants`：确认模式中用户授予的可执行程序权限。

API Key 和 Agent 提示词位于本地加密 Vault 内；同步时和全部 Vault 数据一起加密。AI 操作审计记录、完整终端输出、完整 AI 响应以及应用日志不进入 Vault。

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
本地 vault.json（AES-256-GCM 业务信封）
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

## 6. 凭据与旧文件

`sync.json` 只保存 provider、远端绑定、同步基线、设备 ID 与自动同步开关。应用只支持一个同步绑定，系统凭据库使用固定账户 `sync-v1:token` 与 `sync-v1:derived-key` 保存 Token 和派生同步密钥；同步密码仅在首次绑定、重新输入或轮换时存在于进程内，不会持久化。

`<程序目录>/data/local.key`、明文 `vault.json`、`vault.db` 及旧含凭据的 `sync.json` 不受支持，当前程序不会读取、修改或迁移这些文件。

完整的同步流程、冲突策略、远端 API 边界和验收清单见 [config-security-sync.md](config-security-sync.md)。