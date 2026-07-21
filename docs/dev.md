# MJJSSH 开发文档

## 技术栈

| 层级 | 技术 | 版本 |
|------|------|------|
| 桌面框架 | Tauri | 2.x |
| 前端框架 | Vue 3 | Composition API |
| 前端语言 | TypeScript | 6.x |
| 构建工具 | Vite | 8.x |
| UI 组件库 | Naive UI | 2.x |
| 状态管理 | Pinia | 3.x |
| SSH 客户端 | russh | 0.62 |
| 本地持久化 | JSON Vault | - |
| 云同步加密 | AES-256-GCM + Argon2id | - |
| 终端模拟 | xterm.js | 6.x |

---

## 项目结构

```
my-ssh/
├── docs/                          # 文档
│   ├── db.md                      # Vault 存储设计
│   ├── cloud-sync.md              # 云同步设计
│   └── dev.md                     # 开发文档
├── my-ssh-frontend/
│   ├── src/
│   │   ├── App.vue                # 主界面
│   │   ├── main.ts                # 入口
│   │   ├── router/                # 路由
│   │   ├── components/
│   │   │   ├── Terminal.vue       # SSH 终端
│   │   │   ├── ConnectionDialog.vue  # 连接状态弹窗
│   │   │   ├── KeysView.vue       # 密钥管理
│   │   │   └── SftpView.vue       # SFTP 文件管理
│   │   ├── stores/
│   │   │   ├── vault.ts           # 凭证库状态
│   │   │   └── session.ts         # SSH 会话状态
│   │   └── types/                 # TypeScript 类型
│   └── src-tauri/
│       ├── src/
│       │   ├── commands/          # Tauri 命令
│       │   │   ├── vault.rs       # 凭证库操作
│       │   │   ├── ssh.rs         # SSH 连接
│       │   │   ├── sftp.rs        # SFTP 文件管理
│       │   │   ├── sync.rs        # 云同步命令
│       │   │   └── clipboard.rs   # 剪贴板操作
│       │   ├── vault/             # JSON Vault 存储
│       │   │   ├── store.rs       # 原子读写与 CRUD
│       │   │   └── models.rs      # 数据模型
│       │   ├── sync/              # GitHub Gist/Gitee 云同步
│       │   │   ├── crypto.rs      # Argon2id 与 AES-256-GCM
│       │   │   ├── github_gist.rs # GitHub Gist 提供方
│       │   │   ├── gitee_snippet.rs # Gitee 代码片段提供方
│       │   │   └── service.rs     # 同步服务与冲突处理
│       │   ├── ssh/               # SSH 模块
│       │   │   └── client.rs      # russh 客户端
│       │   ├── state.rs           # 应用状态
│       │   └── lib.rs             # 入口
│       └── Cargo.toml
└── readme.md
```

---

## 开发规范

### 代码风格

- **TypeScript**：严格模式，使用 `interface` 定义类型
- **Vue**：使用 Composition API + `<script setup>` 语法
- **Rust**：遵循 rustfmt 默认格式
- **命名**：
  - 前端：camelCase（变量、函数）、PascalCase（组件、类型）
  - 后端：snake_case（变量、函数）、PascalCase（结构体、枚举）

### Git 提交规范

```
<type>(<scope>): <subject>

type:
  feat     - 新功能
  fix      - 修复
  refactor - 重构
  style    - 样式
  docs     - 文档
  chore    - 构建/工具
```

### 文件组织

- 每个组件一个文件，职责单一
- 共享类型放 `types/index.ts`
- 状态管理按功能拆分（vault、session）
- Tauri 命令按功能拆分（vault、ssh、sftp）

### 主题适配

- 应用支持亮色与暗色主题；主题状态由 `App.vue` 的 `NConfigProvider` 管理，亮色主题使用 `null`，暗色主题使用 Naive UI 的 `darkTheme`。
- 所有 Naive UI 全局配色必须通过带 `GlobalThemeOverrides` 类型的亮色/暗色 `theme-overrides` 配置；需要同步 `body` 的全局样式时，在 `NConfigProvider` 内使用 `NGlobalStyle`。
- 自定义组件不得为结构性颜色硬编码单一主题值。背景、卡片、边框、文字、弱化文字、悬停与主色分别使用 `--app-base`、`--app-surface`、`--app-border`、`--app-text`、`--app-muted`、`--app-hover`、`--app-accent` CSS 变量。
- 状态语义色（例如成功、警告、错误）可以使用固定颜色，但必须在亮色和暗色背景下保持可读；不要通过内联 `style` 固定普通文字或组件颜色。
- 新增或修改自定义页面、弹窗、侧栏与面板时，必须分别检查亮色和暗色主题下的背景、边框、正文、占位文字、图标及 hover/focus 状态。

### 交互确认规范

- 所有由按钮触发、会删除数据、覆盖数据、断开连接或修改安全设置的二次确认，必须使用 Naive UI 的 `NPopconfirm`，并将其包裹在按钮触发器上。
- 禁止使用浏览器原生的 `window.confirm`、`window.alert` 或 `window.prompt`。确认提示必须紧邻触发操作，说明不可逆或覆盖影响，并提供明确的确认与取消文案。

示例：

```vue
<n-popconfirm
  positive-text="确认删除"
  negative-text="取消"
  @positive-click="handlePositiveClick"
>
  <template #trigger>
    <n-button type="error">删除</n-button>
  </template>
  此操作不可恢复。
</n-popconfirm>
```

### 性能开发规范

- **启动关键路径**：首页只初始化 Vault 并加载主机配置；不得阻塞启动以检查云同步状态或请求同步密码。密钥列表仅在密钥管理页或选择密钥/证书认证时首次加载，并复用 store 缓存。
- **按需加载**：终端、SFTP、密钥管理和低频弹窗保持异步组件边界。新增首页非必需功能时，优先采用 `defineAsyncComponent` 或等价的懒加载方案。
- **终端就绪协议**：必须在 `Terminal` 注册 `ssh-data` listener 后通知 session store 终端已就绪，随后才能调用 `connect_ssh`。不得恢复固定延时等待，避免首批 SSH 输出丢失。
- **终端输出**：保持后端的每会话输出合批与有界队列；不得在输出链路中引入无界 channel、逐包同步 IPC 或跨 session 合并数据。前端每个终端复用一个流式 `TextDecoder`。
- **会话并发**：从 `SessionManager` 查到 `Arc<SshSession>` 后立即释放 session map 锁；禁止持有该锁跨 SSH、SFTP 或网络 `await`。
- **SFTP**：复用 SSH session 缓存的 `SftpSession`，不要为单次文件操作新建 subsystem。上传、下载必须保持流式读写和有界并发；当前每会话并发数为 2。传输任务历史最多保留 100 条。覆盖已有本地或远端文件前必须经用户确认。
- **轮询与监听器**：stats 使用请求结束后再调度的 `setTimeout`，活跃页面每 10 秒一次，页面隐藏时暂停。所有 Tauri、DOM listener 和 timer 都必须保存清理函数，并在组件卸载时释放。
- **内存边界**：xterm scrollback 保持 5000 行，除非有压测数据支持调整。新增队列、缓存或后台任务时必须定义容量、淘汰和关闭策略。
- **性能验证**：涉及启动、SSH 输出、SFTP、缓存或轮询的改动，至少执行 `npm run build --prefix my-ssh-frontend`、`cargo check --manifest-path my-ssh-frontend/src-tauri/Cargo.toml` 和 `git diff --check`。真实吞吐、延迟和内存收益须在可控 SSH/SFTP 环境中测量，不可只凭构建结果声称性能提升。

---

## Vault 与云同步



- 本地唯一业务文件为 `<程序目录>/data/vault.json`。未启用云同步时，本地 Vault 为明文 JSON，启动和日常 SSH 使用不要求密码。
- 不使用固定默认密码、随机本地加密密钥或平台特定系统密钥库。
- 启用云同步后，用户输入同步密码；应用对完整 Vault JSON 执行 Argon2id 密钥派生和 AES-256-GCM 整体加密，再上传 GitHub Gist/Gitee 私有片段。
- 同步密码仅用于云端副本，不影响本地 SSH 凭证、不会上传且不可找回。
- `data/sync.json` 默认保存 provider、远端绑定、token 和 Base64 编码的 Argon2id 派生 AES key，以便后续同步免输入；绝不保存原始同步密码。日常同步复用远端 Vault 的随机 KDF salt 并更新 AES-GCM nonce；只有修改同步密码时才轮换 salt 和派生 key。其他已配置设备在改密后必须验证新密码以刷新本机派生 key。该文件不使用系统凭据库；不得写入日志、远端片段、`vault.json` 或浏览器持久化存储。关闭同步或删除远端数据会删除该文件。
- 同步期间 Argon2id 每次用户输入密码后仅派生一次密钥；不得在每次 SSH 连接或读取配置时重复派生。
- 完整设计见 [db.md](db.md) 和 [cloud-sync.md](cloud-sync.md)。

---

## SSH 连接流程

```
1. 前端创建并激活终端页签
2. Terminal 挂载并注册 ssh-data 事件监听
3. Terminal 通知 session store 已就绪
4. 前端调用 connect_ssh(profileId, sessionId)
5. 后端从 Vault 读取凭证
6. 建立 SSH 连接（russh），创建 channel 并请求 PTY
7. 后端通过 channel 读写数据，按 session 合批发送终端输出
```

### 关键点

- sessionId 由前端生成，后端使用同一个
- `connect_ssh` 必须等待 Terminal 就绪，不能依赖固定延迟
- Terminal 组件使用 `v-show` 保持存活
- 事件监听器在 `onMounted` 注册，`onBeforeUnmount` 清理
- 后端输出队列有容量上限；前端消费变慢时会对 SSH 数据处理施加背压

---

## 窗口管理

| 窗口类型 | 父窗口 | 说明 |
|----------|--------|------|
| 主窗口 | - | 页签 + 终端 + 首页 |
| SFTP 窗口 | 主窗口 | 浮动子窗口 |

### SFTP 窗口

- 通过 `parent()` 设置父窗口关系
- 跟随父窗口移动
- 父窗口关闭时自动关闭
- 通过 URL 传递 sessionId 等参数

---

## 前端状态管理

### vault store

```typescript
{
  isReady: boolean         // 本地 Vault 是否已加载
  profiles: SshProfileView[] // 主机列表
  sshKeys: SshKeyView[]    // 密钥列表（按需加载并缓存）
  loading: boolean         // 加载状态
  error: string | null     // 错误信息
}
```

### session store

```typescript
{
  tabs: TabInfo[]          // 页签列表
  activeTabId: string | null // 当前激活页签
  sessions: SessionInfo[]  // SSH 会话列表
}
```

---

## Tauri 命令列表

### vault.rs

| 命令 | 说明 |
|------|------|
| `init_vault` | 初始化本地 Vault |
| `list_profiles` | 列出主机 |
| `create_profile` | 创建主机 |
| `update_profile` | 更新主机 |
| `delete_profile` | 删除主机 |
| `list_keys` | 列出密钥 |
| `create_key` | 创建密钥 |
| `update_key` | 更新密钥 |
| `delete_key` | 删除密钥 |

### sync.rs

| 命令 | 说明 |
|------|------|
| `get_sync_status` | 获取同步配置与状态 |
| `enable_github_gist_sync` | 按固定名称自动查找或创建 GitHub Gist 同步副本 |
| `enable_gitee_snippet_sync` | 按固定名称自动查找或创建 Gitee 私有代码片段同步副本 |
| `upload_sync_vault` | 上传本地 Vault |
| `download_sync_vault` | 下载远端 Vault |
| `resolve_sync_conflict` | 保留本地或接受远端以解决冲突 |
| `change_sync_password` | 修改远端同步副本的密码 |
| `disable_sync` | 删除本地同步配置 |
| `delete_remote_sync_vault` | 删除远端同步副本 |

### ssh.rs

| 命令 | 说明 |
|------|------|
| `connect_ssh` | 建立 SSH 连接 |
| `disconnect_ssh` | 断开 SSH 连接 |
| `write_ssh_data` | 写入 SSH 数据 |
| `resize_ssh` | 调整终端大小 |
| `list_sessions` | 列出活跃会话 |

### sftp.rs

| 命令 | 说明 |
|------|------|
| `open_sftp_window` | 打开 SFTP 窗口 |
| `sftp_get_home_directory` | 获取远端当前用户的家目录，作为 SFTP 初始目录 |
| `sftp_list_files` | 列出文件 |
| `sftp_upload_file` | 上传本地文件并发送进度事件 |
| `sftp_download_file` | 下载远程文件并发送进度事件 |
| `get_default_download_directory` | 获取默认本地下载目录 |
| `get_server_stats` | 获取服务器状态 |

---

## 构建与运行

### 开发

```bash
cd my-ssh-frontend
npm install
npm run dev:tauri
```

### 构建

```bash
npm run tauri build
```

输出：
- `src-tauri/target/release/mjj-ssh.exe`
- `src-tauri/target/release/bundle/nsis/mjj-ssh_0.1.0_x64-setup.exe`

---

## 注意事项

1. **Terminal 组件**：使用 `v-show` 而非 `v-if`，保持终端存活，并保持 5000 行 scrollback 上限
2. **SSH 事件监听**：必须在 `onMounted` 注册，`onBeforeUnmount` 清理；连接必须等待终端就绪通知
3. **SFTP 窗口**：使用 `parent()` 设置父子关系；文件操作复用父 SSH session 的 SFTP subsystem
4. **Vault 与同步**：本地 JSON 写入必须使用临时文件、刷新和原子重命名，并保留备份；云同步只上传整体加密副本，不能上传同步密码或 token
5. **数据目录**：存储在 `<程序目录>/data/`，不在 C 盘
6. **后台工作**：不可见页面不轮询服务器状态；新增 listener、timer、缓存或队列时必须定义清理和容量边界
