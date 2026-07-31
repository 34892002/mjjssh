# MJJSSH 开发文档
> 阅读 docs 了解项目背景和开发规范

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
- Teleport 组件可继承 `NConfigProvider` 主题，但不能继承 DOM 祖先的 `--app-*` 变量；需要时用 `useThemeVars()` 显式映射，并保留默认 `to`。
- 嵌套模态使用官方 `v-model:show` 与 `preset` 结构，保留默认焦点管理；宽度等根节点样式通过 `NModal` 的 `style` 设置，避免 scoped CSS 在 Teleport 后失效。

### 图标与浮动弹窗

- 优先使用项目已引入的 `lucide-vue` 图标表达关闭、添加、刷新、删除等常见操作；图标按钮必须提供 `title` 或 `aria-label`，不要用文字按钮替代已有的通用图标。
- 在 `NModal` 等模态窗口内，需要临时展示筛选、选择或编辑内容且不应撑开底层布局时，使用 `FloatingPanel`。调用方负责内容和状态，组件负责遮罩、关闭入口与焦点语义。

### 交互确认规范

- 所有由按钮触发、会删除数据、覆盖数据、断开连接或修改安全设置的二次确认，必须使用 Naive UI 的 `NPopconfirm`，并将其包裹在按钮触发器上。
- 禁止使用浏览器原生的 `window.confirm`、`window.alert` 或 `window.prompt`，也不得通过未限定的 `confirm`、`alert`、`prompt` 调用它们。
- 二次确认使用 `NPopconfirm` 或 `NModal`；成功、失败、警告和信息反馈使用 `NAlert`、`NMessage` 或 `NNotification`。需要保留详细结果（例如文件导出路径）时，优先使用可关闭的 `NAlert`。
- 确认提示必须紧邻触发操作，说明不可逆或覆盖影响，并提供明确的确认与取消文案。

确认示例：

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

反馈示例：

```vue
<n-alert title="Success 类型" type="success" closable>
  Leave it till tomorrow to unpack my case
</n-alert>
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



- 本地唯一业务文件为 `<程序目录>/data/vault.json`，其业务 JSON 使用 AES-256-GCM 加密；`vault.json.bak`、临时文件和冲突中的本地快照必须使用相同保护。
- 首次创建 Vault 时生成随机本地数据密钥，保存到 Windows Credential Manager 或 macOS Keychain。不得创建可复制的 `local.key`，也不得在缺失密钥时覆盖既有加密 Vault。
- 启用云同步后，用户输入 Token 与同步密码；应用对完整 Vault JSON 执行 Argon2id 密钥派生和 AES-256-GCM 整体加密，再上传 GitHub Gist/Gitee 私有片段。
- Token 和 Base64 编码的派生同步密钥以固定账户名保存于系统凭据管理器：`sync-v1:token` 与 `sync-v1:derived-key`。应用仅支持一个同步绑定，重复同步覆盖固定账户；关闭同步或删除远端库会清除这两项同步凭据。`data/sync.json` 只保存 provider、远端绑定、同步基线、设备 ID 和自动同步开关；状态接口及前端不得返回、缓存或传递 Token。
- 同步密码仅用于首次绑定、重新验证和轮换，不影响本地 SSH 凭证、不会上传或持久化且不可找回。日常同步复用远端 Vault 的随机 KDF salt 并更新 AES-GCM nonce；只有修改同步密码时才轮换 salt 和派生 key。
- 同步期间 Argon2id 每次用户输入密码后仅派生一次密钥；不得在每次 SSH 连接或读取配置时重复派生。
- Windows 与 macOS 上的 `local_security` 平台条件单元测试会使用随机临时账户验证真实 Credential Manager/Keychain 的写入、读回、删除，以及读回密钥的 AES-256-GCM 加解密；不要把它改成固定应用账户。
- 新项目不支持明文 Vault 或旧的含凭据 `sync.json`。完整设计见 [db.md](db.md) 和 [config-security-sync.md](config-security-sync.md)。

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
| `change_sync_password` | 修改远端同步副本的密码 |
| `resolve_sync_conflict` | 保留本地或接受远端以解决冲突 |
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
- `src-tauri/target/release/` 按照平台生成对应安装文件

### Release 编译规则与启动崩溃记录

#### 编译规则

- `my-ssh-frontend/src-tauri/Cargo.toml` 的 release profile 必须保持 `lto = false`、`codegen-units = 16`；不要恢复 `lto = "thin"` 与 `codegen-units = 1` 的组合。
- 正式桌面包必须使用 `npm run tauri -- build`（或 `npm run tauri build`）构建，确保生产前端资源被嵌入；不得用裸 `cargo build --release` 作为安装版验证。
- Windows 发布验证必须先卸载旧版再安装新包，至少观察进程运行 10 秒，并检查 `<exe目录>/data/logs/startup.log` 与 Windows WER。
- 仅 debug 运行、裸 `cargo build --release` 或观察 5 秒，都不能作为 release 安装版通过的依据。
- GitHub Actions、Windows、macOS 和 Linux 的正式构建都使用上述 Cargo release profile，不得为单个平台恢复高风险优化组合。
- Windows 可执行文件必须在 `src-tauri/build.rs` 保持 `/STACK:8388608` 链接参数。构建后用 `llvm-objdump --private-headers src-tauri/target/release/mjj-ssh.exe` 确认 `SizeOfStackReserve` 为 `0x800000`（8 MiB）；不得回退为默认的 1 MiB。
- SFTP 的“使用默认应用编辑”必须启动编辑器，而非按文件关联执行“打开”。临时副本目录由 `app.path().app_local_data_dir().join("remote-edit")` 解析，这是 Windows、macOS 和 Linux 共用的 Tauri 应用本地数据目录；不得使用 `$EXE`（Windows 不支持）或 `$RESOURCE`（各平台路径语义不同）。Windows 优先使用 `ShellExecuteW` 的 `edit` 动词；若扩展名未注册该动词，则直接启动 `notepad.exe`，不得回退到 `open`。macOS 使用 `open -t`；Linux 使用 `VISUAL`，未设置时使用 `EDITOR`，两者均未设置时显示配置编辑器的错误。不得使用 `tauri-plugin-opener`、`open` 动词或 `xdg-open` 回退。

#### 事故记录

- `v0.2.2` Windows 安装版曾在启动阶段退出，Windows WER 异常代码为 `0xc00000fd`（栈溢出）。
- 对照验证表明：`lto = "thin"`、`codegen-units = 1` 会触发该问题；改为 `lto = false`、`codegen-units = 16` 后，带生产前端和系统托盘的 NSIS 安装版在 `D:\\soft\\MJJSSH` 启动并持续运行超过 10 秒。
- 2026-07-31，SFTP 的“编辑文本”和“使用默认应用编辑”在 release exe 中均触发 Windows WER `APPCRASH`：异常码 `0xc00000fd`，崩溃偏移从 `0x000000000103a937` 变为 `0x00000000010363f7`。后者位于 `mjj-ssh.exe` 的入口点 `0x0000000001036390` 附近，证实为主线程栈耗尽，而非 SFTP、CodeMirror 或 ShellExecute 的可捕获错误。通过 `build.rs` 的 `/STACK:8388608` 将 PE `SizeOfStackReserve` 从 1 MiB 提高到 8 MiB 后，两个入口不再使进程退出。
- “使用默认应用编辑”不依赖 opener capability 或本地路径打开权限。若系统未配置可用编辑器，必须保留临时副本并显示明确错误；Windows 在缺少 `edit` 动词时仅可回退到 `notepad.exe`，不得回退到 `open`；Linux 不得回退到 `xdg-open`。

### 更新版本

发布新版本时，将以下文件中的版本号统一更新为目标版本（配置文件使用不带 `v` 的版本号，设置页显示使用带 `v` 的版本号）：

- `my-ssh-frontend/src-tauri/tauri.conf.json`：安装包与应用版本。
- `my-ssh-frontend/src-tauri/Cargo.toml`：Rust crate 版本。
- `my-ssh-frontend/src-tauri/Cargo.lock`：仅更新 `name = "mjj-ssh"` 包条目的版本；不要改动其他依赖的同名版本号。
- `my-ssh-frontend/package.json`：前端包版本。
- `my-ssh-frontend/package-lock.json`：根包及 `packages[""]` 的版本。
- `my-ssh-frontend/src/App.vue`：系统设置中显示的 `v<版本号>`。

更新后至少核对上述位置的版本一致性，并执行：

```bash
npm run build --prefix my-ssh-frontend
cargo fmt --manifest-path my-ssh-frontend/src-tauri/Cargo.toml -- --check
cargo check --manifest-path my-ssh-frontend/src-tauri/Cargo.toml
git diff --check
```

---

## 注意事项

1. **Terminal 组件**：使用 `v-show` 而非 `v-if`，保持终端存活，并保持 5000 行 scrollback 上限
2. **SSH 事件监听**：必须在 `onMounted` 注册，`onBeforeUnmount` 清理；连接必须等待终端就绪通知
3. **SFTP 窗口**：使用 `parent()` 设置父子关系；文件操作复用父 SSH session 的 SFTP subsystem
4. **Vault 与同步**：本地 JSON 写入必须使用临时文件、刷新和原子重命名，并保留备份；云同步只上传整体加密副本，不能上传同步密码或 token
5. **数据目录**：存储在 `<程序目录>/data/`，不在 C 盘
6. **后台工作**：不可见页面不轮询服务器状态；新增 listener、timer、缓存或队列时必须定义清理和容量边界
