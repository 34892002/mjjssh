# 组件文档

本文档记录 `my-ssh-frontend` 中的自定义 Vue 组件，以及应用级组件的使用方式。

> 当前项目未通过 `app.component()` 注册全局 Vue 组件。除根组件 `App.vue` 外，其余组件均由使用方显式导入。Naive UI 和 Lucide 图标属于第三方依赖，不在本清单内。

| 组件名 | 组件功能 | 组件路径 |
| --- | --- | --- |
| App | 应用根组件；负责主界面布局、主机与密钥视图切换、SSH 页签、SFTP 面板及全局弹窗状态协调。 | `my-ssh-frontend/src/App.vue` |
| ActionDialog | 通用应用级操作弹窗，支持文本输入和确认操作两种模式，可用于替代浏览器原生提示框。 | `my-ssh-frontend/src/components/ActionDialog.vue` |
| AiChatPanel | 按 SSH 会话展示 AI 对话侧栏，负责对话、Agent 选择和流式结果展示；AI 服务与 Agent 管理由设置页负责。 | `my-ssh-frontend/src/components/AiChatPanel.vue` |
| AiSettings | 设置 → AI 页签的按需加载组件，管理 AI 服务配置、连接测试与 Agent 配置。 | `my-ssh-frontend/src/components/AiSettings.vue` |
| ConnectionDialog | 展示 SSH 连接、认证、成功或失败的进度与操作入口。 | `my-ssh-frontend/src/components/ConnectionDialog.vue` |
| KeysView | 管理 SSH 私钥与证书，包括创建、编辑和删除。 | `my-ssh-frontend/src/components/KeysView.vue` |
| PermissionsDialog | 编辑远程文件或目录的 Unix 读、写、执行权限，并输出八进制权限值。 | `my-ssh-frontend/src/components/PermissionsDialog.vue` |
| SftpView | 提供远程 SFTP 文件浏览、排序、上传、文件操作及右键菜单。 | `my-ssh-frontend/src/components/SftpView.vue` |
| Terminal | 基于 xterm.js 渲染 SSH 交互终端，处理终端输入、输出与尺寸同步。 | `my-ssh-frontend/src/components/Terminal.vue` |
| TransferPanel | 按 SSH 会话展示 SFTP 上传、下载任务的进度与历史，并允许修改默认下载目录。 | `my-ssh-frontend/src/components/TransferPanel.vue` |

## 组件加载说明

- 应用入口 `my-ssh-frontend/src/main.ts` 通过 `createApp(App)` 创建并挂载根组件。
- `App.vue` 显式导入并组合使用 `ActionDialog`、`AiChatPanel`、`AiSettings`、`ConnectionDialog`、`KeysView`、`PermissionsDialog`、`SftpView` 和 `Terminal`；`SftpView` 内部按需加载 `TransferPanel`。
- 路由 `my-ssh-frontend/src/router/index.ts` 的 `/` 与 `/sftp` 当前均指向 `App` 根组件。
