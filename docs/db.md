# MJJSSH 数据库设计

SQLite 数据库，存储位置：`<程序目录>/data/vault.db`

## 加密方案

```
主密码 + salt → Argon2id (65536 iterations, 3 blocks, 4 parallelism) → AES-256-GCM 密钥
                                                                          ↓
                                              加密 credential / private_key / cert_data 字段
```

- 默认密码：`LuckyMJJ`（首次启动自动使用）
- `local.key` 文件格式：`[16字节 salt][32字节派生密钥]`
- 启动时读取 `local.key` 直接解锁，无需输入密码
- 主密码仅在 S3 同步场景使用

---

## 表结构

### profiles（SSH 主机配置）

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | TEXT | PK | UUID |
| `name` | TEXT | ✅ | 连接名称 |
| `host` | TEXT | ✅ | 主机地址 |
| `port` | INTEGER | ✅ | SSH 端口，默认 22 |
| `username` | TEXT | ✅ | 登录用户名 |
| `auth_type` | TEXT | ✅ | `password` / `key` / `certificate` |
| `credential` | BLOB | | AES-GCM 加密后的密码（仅 password 认证） |
| `key_id` | TEXT | | FK → ssh_keys.id（key/certificate 认证） |
| `group_name` | TEXT | | 分组名称 |
| `created_at` | TEXT | ✅ | 创建时间 (RFC3339) |
| `updated_at` | TEXT | ✅ | 更新时间 (RFC3339) |

### ssh_keys（SSH 密钥/证书）

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | TEXT | PK | UUID |
| `name` | TEXT | ✅ | 密钥名称 |
| `key_type` | TEXT | ✅ | `key`（私钥）/ `certificate`（证书） |
| `private_key` | BLOB | ✅ | AES-GCM 加密后的私钥内容 |
| `cert_data` | BLOB | | AES-GCM 加密后的证书内容（仅 certificate） |
| `created_at` | TEXT | ✅ | 创建时间 (RFC3339) |
| `updated_at` | TEXT | ✅ | 更新时间 (RFC3339) |

---

## 关联关系

```
profiles.key_id ──FK──→ ssh_keys.id
```

| 认证方式 | 密码存储位置 | 私钥存储位置 | 证书存储位置 |
|----------|-------------|-------------|-------------|
| password | profiles.credential | - | - |
| key | - | ssh_keys.private_key | - |
| certificate | - | ssh_keys.private_key | ssh_keys.cert_data |

---

## E-R 图

```
┌──────────────────────────┐       ┌──────────────────────────┐
│        profiles          │       │        ssh_keys          │
├──────────────────────────┤       ├──────────────────────────┤
│ id (PK)                  │       │ id (PK)                  │
│ name                     │       │ name                     │
│ host                     │       │ key_type                 │
│ port                     │       │ private_key (encrypted)  │
│ username                 │       │ cert_data (encrypted)    │
│ auth_type                │       │ created_at               │
│ credential (encrypted)   │       │ updated_at               │
│ key_id ──────────────────┼──FK──→│                          │
│ group_name               │       │                          │
│ created_at               │       │                          │
│ updated_at               │       │                          │
└──────────────────────────┘       └──────────────────────────┘
```
## 默认密码检测

`config` 表存储 `init` 字段（加密的 "LuckyMJJ" 字符串）。

**检测流程：**
1. 读取 `config.init`（加密数据）
2. 读取 `local.key` 中的 salt
3. 用 "LuckyMJJ" + salt 派生密钥
4. 比对派生密钥与存储密钥
5. 用派生密钥解密 `config.init`
6. 如果解密成功 → 默认密码；失败 → 用户已修改密码

---

## 本地密钥文件

**路径：** `<程序目录>/data/local.key`

**格式：** 48 字节二进制

```
[0..16]   salt      (16 bytes)  - Argon2id 盐值
[16..48]  key       (32 bytes)  - Argon2id 派生的 AES-256 密钥
```

**默认密码检测：** 用 "LuckyMJJ" + salt 重新派生密钥，与存储的密钥比对。相同则为默认密码。

---

## 密码修改流程

```
1. 验证旧密码（派生密钥比对）
2. 生成新 salt + 新密钥
3. 用旧密钥解密 profiles.credential → 用新密钥重新加密
4. 用旧密钥解密 ssh_keys.private_key → 用新密钥重新加密
5. 用旧密钥解密 ssh_keys.cert_data → 用新密钥重新加密
6. 写入新 local.key
```
