# OpenClaw Manager 仓库说明

## 作用范围

这份文件是给后续代理/维护者使用的仓库工作说明。
分析基于当前这个干净 clone：

- 分支：`main`
- 提交：`50b01c3`
- 远程：`https://github.com/mazycc/openclaw-manager.git`

本文重点说明：

- 这个项目本质上是什么
- 运行时到底怎么工作
- 数据写到哪里
- 前端和 Tauri/Rust 后端如何交互
- release 是怎么产出的
- 当前仓库里哪些细节存在风险或不一致

## 本地验证状态

当前工作区已经验证：

- `node -v` -> `v24.11.1`
- `npm -v` -> `11.6.2`
- `npm ci` 成功
- `npm run build` 成功

当前工作区尚未验证：

- 没有安装 `cargo`
- 没有运行 `npm run tauri:dev`
- 没有运行 `npm run tauri:build`

结论：

- React/Vite 这一层在当前环境可正常构建
- Rust/Tauri 打包层在当前环境还不能本地构建

## 这个项目是什么

OpenClaw Manager 是一个桌面 GUI，用来管理 OpenClaw 的安装和运行。

这个仓库本身并不把 OpenClaw runtime 作为源码直接内置在项目里。
它的核心定位，是通过 shell 调用去管理一个外部安装的 OpenClaw CLI。

Manager 的核心职责包括：

- 检查 Node.js、Git、OpenClaw 是否已安装
- 缺失时安装 Node.js 和 OpenClaw
- 初始化 `~/.openclaw`
- 启动、停止、重启、监控 OpenClaw gateway
- 用表单编辑 OpenClaw 配置，而不是手改 JSON
- 管理 AI provider 和模型
- 管理消息 channels
- 管理 MCP server 和 mcporter
- 通过 Clawhub 管理 skills
- 管理 agents、bindings、subagent defaults
- 测试环境、AI 连通性、channel 状态
- 通过 Tauri updater 做应用自更新

## 高层架构

这是一个双层桌面应用：

- 前端：React 18 + TypeScript + Vite + Tailwind + Framer Motion
- 桌面/原生层：Rust + Tauri 2

前端负责页面渲染和输入收集。
Rust 层负责文件系统操作、进程控制、shell 执行、安装逻辑、配置写入和更新处理。

## 顶层入口

- `src/main.tsx`
  - React 启动入口
  - 引入全局样式并初始化前端日志
- `src/App.tsx`
  - 主应用外壳
  - 懒加载各个页面模块
  - 触发环境检查、服务轮询、OpenClaw 更新检查、Manager OTA 检查
- `src/lib/tauri.ts`
  - 带日志的类型化 `invoke()` 包装
  - 集中定义很多 Tauri 命令对应的 TypeScript 类型
  - 当前并没有在所有页面统一使用，部分页面仍然直接调用 `invoke(...)`
- `src-tauri/src/main.rs`
  - Tauri 应用入口
  - 注册插件
  - 通过 `invoke_handler` 向前端暴露 Rust 命令

## 运行时模型

核心运行模式基本是：

1. React 页面渲染一个表单或按钮
2. 用户操作触发一个 Tauri command
3. Rust command 读取/写入配置，调用 shell 工具，或管理进程
4. 结果返回给页面
5. 页面刷新本地状态

这不是一个很重的全局状态管理架构。
大部分页面都自己维护取数和修改逻辑。
仓库里有 Zustand，但用得不重：

- `src/stores/appStore.ts`
- `src/hooks/useService.ts`

多数业务状态仍然直接留在页面组件内部。

## 前端架构

### 页面加载模型

`src/App.tsx` 通过懒加载加载主要页面：

- Dashboard
- AIConfig
- Channels
- MCP
- Skills
- Settings
- Logs
- Agents

这件事的重要性在于：

- 能减少首屏 JS 体积
- 和 v0.0.19 release note 里提到的 lazy chunking、startup speed 优化是对应的
- `npm run build` 的产物也能看到 `dist/assets/*.js` 被拆成多个 chunk

### 应用启动行为

前端启动后会做几轮延迟检查：

- 立即：环境检查
- 1 秒后：OpenClaw secure version 检查
- 2 秒后：OpenClaw 更新检查
- 6 秒后：Manager OTA 自更新检查
- 每 3 秒：服务状态轮询

这是一个明确的“先渲染，再后台加载数据”的模式。

### 前端日志

`src/lib/logger.ts` 维护一个浏览器侧的内存日志存储。

要注意区分：

- `Logs` 页面里看到的一部分，是前端应用自己的内存日志
- OpenClaw 服务日志则是通过 Rust 的 `get_logs` 拉回来的

这两者不是同一套日志。

## 后端架构

Rust 代码主要组织在：

- `src-tauri/src/commands`
- `src-tauri/src/utils`
- `src-tauri/src/models`

### Command 模块

- `commands/installer.rs`
  - 环境检测
  - Node.js 安装
  - OpenClaw 安装
  - 配置初始化
  - 卸载
  - OpenClaw 更新
- `commands/service.rs`
  - gateway 状态
  - 启动/停止/重启
  - 健康检查
  - 日志获取
  - 端口清理
- `commands/process.rs`
  - OpenClaw 版本检查
  - secure version 检查
  - 端口检查
  - Ollama 检测与模型拉取
- `commands/config.rs`
  - 主要配置中枢
  - AI providers
  - channels
  - MCP
  - agents
  - bindings
  - gateway 配置
  - import/export
  - subagent defaults
  - 以及很多零散配置接口
- `commands/diagnostics.rs`
  - doctor
  - AI 测试
  - channel 测试
  - system info
  - login 流程
- `commands/skills.rs`
  - Clawhub 检测/安装/卸载
  - skill 安装/卸载

### Utility 模块

- `utils/platform.rs`
  - 统一路径规则
  - OS 检测
- `utils/shell.rs`
  - 命令执行辅助
  - PATH 扩展逻辑
  - OpenClaw 二进制定位
  - 在 CLI 调用里注入 gateway token
- `utils/file.rs`
  - 文件和 env 辅助逻辑
- `utils/log_sanitizer.rs`
  - 对日志中的敏感值做掩码/清洗

## 实际数据存储位置

这个 Manager 主要修改的是用户 home 目录下的文件。

`src-tauri/src/utils/platform.rs` 里可以看出关键路径：

- OpenClaw home：
  - Windows：`%USERPROFILE%\\.openclaw`
  - Unix：`~/.openclaw`
- 主配置：
  - `~/.openclaw/openclaw.json`
- env 文件：
  - `~/.openclaw/env`
- manager 配置：
  - `~/.openclaw/manager.json`
- MCP 安装目录：
  - `~/.openclaw/mcps`
- manager MCP 配置：
  - `~/.openclaw/mcps.json`
- mcporter 配置：
  - `~/.mcporter/mcporter.json`

所以这个仓库本质上更像是“外部状态控制器”，而不是一个只依赖仓库内部数据库/状态的自包含应用。

## 核心逻辑原理

### 1. OpenClaw CLI 才是运行时动作的真实执行者

Manager 并没有重新实现 gateway 行为。
很多服务控制和检查逻辑，最终都是 shell 到 `openclaw` 命令。

例如：

- 服务健康检查：`openclaw gateway health`
- 停止服务：`openclaw gateway stop`
- 查看日志：`openclaw logs --limit N`
- 环境诊断：`openclaw doctor`
- AI 测试：`openclaw agent --local ...`
- channel 状态：`openclaw channels status`
- 配置初始化：`openclaw config set ...`

设计上的后果：

- 如果 OpenClaw CLI 的行为变了，这个 Manager 可能会出问题
- 即使这个仓库的前端完全没改，也一样可能被外部 CLI 行为变化影响

### 2. GUI 进程通常拿不到和终端完全一致的 PATH

`src-tauri/src/utils/shell.rs` 里明确在处理 PATH 问题。

它会扩展 PATH，并探测这些常见安装位置：

- `openclaw`
- Node.js
- nvm/fnm/volta/asdf/mise 路径
- Windows 的 roaming npm/global 目录

这是这个仓库非常关键的一个实现原则：

- 不能假设 GUI 进程继承了和交互式终端完全一致的 PATH

### 3. 配置编辑本质上是结构化 JSON 变更

`commands/config.rs` 会加载 `openclaw.json`，修改指定子树，再整体写回。

例如：

- AI 配置写在 `/models/providers`
- 默认模型写在 `/agents/defaults/models`
- gateway 配置写在 `/gateway`
- manager log level 写在 `/manager/log_level`
- agent bindings 写在顶层 `/bindings`，并兼容旧结构

这意味着未来改动时，必须保证和 OpenClaw CLI 的配置 schema 兼容。

### 4. 向后兼容不是文档说明，而是代码真实处理的一部分

仓库里有多个位置都在兼容旧格式和新格式。

例如：

- agents 列表可能是 object，也可能是 array
- bindings 可能在顶层，也可能在 `agents.bindings`
- gateway log level 有 legacy fallback

如果不了解迁移策略，不要随便“简化”这些路径分支。

### 5. 服务状态判断不是只看端口

`service.rs` 不会只因为端口开着就认定服务正常。

它同时检查：

- `openclaw gateway health`
- 服务端口是否真的在监听

这样可以避免“端口被别的进程占了，但并不是 OpenClaw gateway”这种误判。

### 6. 内建了服务监督逻辑

启动服务时，manager 会：

- 在后台启动 OpenClaw gateway
- 等待端口就绪
- 验证健康状态
- 启动一个 supervisor 线程

supervisor 会周期性重新检查 gateway 健康状态，并在检测到异常退出时尝试重启。

这就是 README/release notes 里 “Service Supervisor” 特性的代码基础。

### 7. 安装逻辑的原则是“优先系统自动化，失败再降级到终端流程”

Installer 设计大致如下：

- Node.js：
  - Windows：优先 `winget`，再退回 `fnm`
  - macOS：使用 Homebrew
  - Linux：按发行版包管理器逻辑处理
- OpenClaw：
  - 通过 `npm install -g openclaw@latest --unsafe-perm` 全局安装

如果没法完全自动安装，UI 还可以引导到终端安装流程。

关键结论：

- 这个打包后的桌面管理器不是一个内置 Node.js + OpenClaw 的离线全集成包
- 它本质上还是一个围绕系统级安装步骤做编排的 GUI

### 8. Manager 更新和 OpenClaw 更新是两套独立系统

这里有两条完全不同的更新链路：

- OpenClaw 更新
  - 由 Rust 处理
  - 用 `npm view openclaw version` 检查 npm registry 版本
  - 用 `npm install -g openclaw@latest` 更新
- Manager 自身更新
  - 通过 Tauri updater plugin 处理
  - 依赖 `latest.json`、签名和 release assets
  - 安装后会重启桌面应用

不要把这两套更新逻辑混为一谈。

## 关键功能区及其工作方式

### Dashboard 和 Setup

Dashboard 是实际操作入口。
如果环境检查发现系统未就绪，Dashboard 会把 Setup 流程嵌进去。

Setup 主要做：

- 检查环境
- 缺失时安装 Node.js
- 缺失时安装 OpenClaw
- 初始化配置目录和默认配置状态

### AI Provider 管理

AI provider 预设是在 Rust 侧 `get_official_providers()` 里硬编码的。
这意味着 release note 里说的“最新模型阵容”不是实时联网获取的，而是版本内置代码。

AI 页面会读写：

- provider base URL
- 脱敏后的 API key
- provider 的模型定义
- 默认主模型
- 可用模型列表

保存流程：

- 前端收集 provider 表单数据
- Rust 归一化后写回 `openclaw.json` 对应 provider 子树

### Channels

Channel 配置主要保存在 `openclaw.json`，但有些仅用于测试的字段会写进 env 文件。

配置代码里支持的 channel 类型包括：

- telegram
- discord
- slack
- feishu
- whatsapp
- imessage
- wechat
- dingtalk

channel 测试依赖 CLI 状态输出，再做文本/JSON 解析。

WhatsApp 登录是一个特殊流程：

- Rust 会启动专门的登录流程
- 可能会启用插件，并驱动终端/脚本逻辑
- 它不只是普通的表单写配置

### MCP

MCP 不完全挂在 `openclaw.json` 下，而是独立管理。

关键位置：

- 安装目录：`~/.openclaw/mcps`
- 配置文件：`~/.openclaw/mcps.json`
- mcporter 配置：`~/.mcporter/mcporter.json`

Manager 支持：

- 手工配置 stdio 模式
- 配置远程 URL
- 通过 Git 仓库 URL 安装
- mcporter 安装/卸载
- 基于插件的 MCP 安装辅助

Git 安装路径是真实存在的，不是占位功能：

- clone 仓库
- 按需 install/build
- 持久化配置

### Skills

Skills 依赖 Clawhub。

Manager 可以：

- 检查 Clawhub 是否存在
- 安装/卸载 Clawhub
- 通过 Clawhub CLI 包装完成 skill 安装/卸载

这也是“GUI 套在外部 CLI 上层”的又一个典型例子。

### Agents 和路由

Agent 配置来自 `openclaw.json`。
代码同时兼容新旧格式。

主要概念包括：

- agents 列表
- 每个 agent 的覆盖配置
- bindings
- subagent 权限
- system prompts
- Telegram 账号路由

Bindings 用来决定某个来信账号/频道应该被路由到哪个 agent。

agent 保存流程不只是改 JSON：

- 还可能创建或引用 workspace、agent 目录
- 同时保留旧布局兼容性

### Diagnostics

Diagnostics 既有直接检查，也有 CLI 驱动检查：

- OpenClaw 是否已安装
- Node 是否已安装
- 配置文件是否存在
- env 文件是否存在
- `openclaw doctor`

AI 连通性测试不是假的 mock。
它会真的通过 OpenClaw 发起调用，并测量实际延迟。

## Release Notes 与代码的对应关系

v0.0.19 的 release notes 提到：

- startup speed optimization
- lazy chunking
- concurrent environment checks
- model lineup refresh
- Ollama fix

这些描述和代码基本是能对上的：

- `src/App.tsx` 里的 `React.lazy(...)` 页面懒加载
- 启动阶段改成延迟异步检查，而不是先把所有查询阻塞完
- `check_environment()` 里用了 `tokio::join!`
- `get_official_providers()` 里更新了硬编码 provider/model 预设

## Release 与打包逻辑

这个仓库的 release 流程主要由这些文件定义：

- `.github/workflows/release.yml`
- `src-tauri/tauri.conf.json`
- Tauri 自身的 bundling 行为

### 本地构建命令

只构建前端：

```bash
npm run build
```

在具备 Rust/Tauri toolchain 的机器上构建桌面包：

```bash
npm run tauri:build
```

更底层的实际过程可以理解为：

- Vite 先构建 `dist/`
- Tauri 再消费 `dist/`
- Rust/Tauri 最终在 `src-tauri/target/.../release/bundle/` 下产出原生安装包

### 为什么一台本地机器打不出所有 release 资产

线上 release 里包含：

- Windows 安装包
- macOS Intel 和 Apple Silicon 包
- Linux 包

一台 Windows 本地机不可能直接把这一整套都产出来。
仓库用的是 GitHub Actions 的原生 OS runner matrix：

- `windows-latest`
- `macos-latest`，目标 `aarch64-apple-darwin`
- `macos-latest`，目标 `x86_64-apple-darwin`
- `ubuntu-22.04`

所以一个 tag 才能产出整套跨平台 release。

### Release 触发方式

workflow 会在下面两种情况下运行：

- push 符合 `v*` 的 tag
- 手动 `workflow_dispatch`

正常发布路径：

1. bump version
2. 提交 commit
3. push 分支
4. 创建类似 `v0.0.19` 的 tag
5. push tag
6. GitHub Actions 创建 draft release
7. matrix 构建并上传资产
8. 最终 job 把 draft 发布出去

### 需要同步的版本文件

最少应保持这两个文件版本一致：

- `package.json`
- `src-tauri/tauri.conf.json`

另外这个文件也应该同步，虽然当前文档没明确强调：

- `src-tauri/Cargo.toml`

现有 `RELEASE.md` 只提到了两个文件，但 Rust crate 自己也有版本号，不建议漂移。

### Workflow 阶段

#### 第一阶段：创建 draft release

`create-release` job：

- checkout 仓库
- 读取 tag 名称
- 通过 `softprops/action-gh-release` 创建 draft GitHub release

一个重要细节：

- release 正文是 workflow 里硬编码的
- 不是根据 changelog 或 commit 自动生成

#### 第二阶段：构建矩阵

`build` job：

- 安装平台依赖
- 安装 Node 22
- 安装目标平台的 Rust toolchain
- 执行 `npm ci`
- 执行 `tauri-apps/tauri-action@v0`

关键输入包括：

- draft release 的 `releaseId`
- `args: --target ...`
- `updaterJsonPreferNsis: true`

Tauri action 会直接把构建产物上传到 draft release。

#### 第三阶段：正式发布

`publish-release` job：

- 在所有 matrix 构建成功后，把 draft release 切换为 published

### 为什么会出现这些具体的 release 资产

你的 release 页面里会出现这么多资产，根本原因是：

- `src-tauri/tauri.conf.json` 里配置了：
  - `"bundle": { "active": true, "targets": "all", "createUpdaterArtifacts": true }`
- GitHub Actions 又分别在多个操作系统和 CPU 目标上构建

两者叠加后，就会产出：

- Windows
  - `.msi`
  - NSIS `.exe`
- macOS
  - `.dmg`
  - `.app.tar.gz`
- Linux
  - `.deb`
  - `.AppImage`
  - `.rpm`

### 线上 release 资产映射

你贴出来的资产和 Tauri 输出关系如下：

- `OpenClaw.Manager_0.0.19_x64-setup.exe`
  - Windows 的 NSIS 安装器
- `OpenClaw.Manager_0.0.19_x64_en-US.msi`
  - Windows 的 MSI 安装器
- `OpenClaw.Manager_0.0.19_x64.dmg`
  - macOS Intel 的磁盘镜像
- `OpenClaw.Manager_0.0.19_aarch64.dmg`
  - macOS Apple Silicon 的磁盘镜像
- `OpenClaw.Manager_x64.app.tar.gz`
  - macOS Intel 的 updater 包/归档
- `OpenClaw.Manager_aarch64.app.tar.gz`
  - macOS Apple Silicon 的 updater 包/归档
- `OpenClaw.Manager_0.0.19_amd64.deb`
  - Linux Debian 包
- `OpenClaw.Manager_0.0.19_amd64.AppImage`
  - Linux AppImage
- `OpenClaw.Manager-0.0.19-1.x86_64.rpm`
  - Linux RPM 包

`.sig` 文件出现的原因，是 Tauri updater 签名被启用了。

`latest.json` 会出现，是因为：

- updater plugin 已启用
- `createUpdaterArtifacts` 已启用
- `tauri-action` 会生成 updater 元数据

### `latest.json` 为什么重要

Manager 的应用内自更新依赖 Tauri updater。

`src-tauri/tauri.conf.json` 里包含：

- updater public key
- updater endpoint

运行时应用会请求：

- `https://github.com/MrFadiAi/openclaw-manager/releases/latest/download/latest.json`

这个 JSON 会告诉应用：

- 最新版本号
- 下载地址
- 签名
- 当前平台对应的 updater 产物

还有一个重要细节：

- `updaterJsonPreferNsis: true` 代表 Windows 的 updater manifest 会优先选 NSIS 安装器，而不是 MSI

### Release 所需 secrets

workflow 依赖这些 secrets：

- `GITHUB_TOKEN`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

如果没有签名私钥，updater 产物和签名文件都会出问题。

### 如果这个 fork 要独立发布自己的更新

当前 updater 配置仍然指向上游：

- `https://github.com/MrFadiAi/openclaw-manager/releases/latest/download/latest.json`

如果 `mazycc/openclaw-manager` 想要完全独立发布，至少要改：

- `src-tauri/tauri.conf.json` 里的 updater endpoint
- 如果换签名私钥，还要同步 updater public key
- release 文档和仓库文档里残留的上游 URL

否则就会出现一种情况：

- 应用是从这个 fork 构建出来的
- 但应用内自更新仍然去追上游 release

## 当前仓库已知问题与漂移

### 1. 缺少 `get_subagent_defaults` command

`src/components/Settings/index.tsx` 里调用了：

- `invoke<SubagentDefaults>('get_subagent_defaults')`

但 `src-tauri/src/main.rs` 并没有暴露这个 command，Rust 侧也没有同名实现。

而现有代码里：

- `get_agents_config()` 已经会返回 `subagent_defaults`
- `save_subagent_defaults()` 已经存在

这看起来是一个真实的前后端 API 不一致问题。

### 2. `uninstall_skill` 被重复注册

`src-tauri/src/main.rs` 里把：

- `skills::uninstall_skill`
- `skills::uninstall_skill`

注册了两次。

大概率不会造成功能性错误，但显然是代码不干净。

### 3. 文档仍残留旧仓库名/旧 URL

仓库文档存在漂移。
例如仍能看到旧仓库名 `openclaw-one-click-installer` 之类的引用。

不要默认 README 和 RELEASE 文档一定是最新的。
涉及实际行为时，应优先信代码和 workflow 配置。

### 4. Updater endpoint 仍然指向上游

见上面的 fork 独立发布说明。

## 后续改动的实操建议

- 把 `src-tauri/src/commands/config.rs` 当成高爆炸半径文件处理
- 新增功能时，至少同时核对这些点：
  - 前端表单字段名
  - Tauri invoke command 名称
  - `main.rs` 里的 Rust handler 注册
  - 写入 `openclaw.json` 的 JSON 结构
  - 是否需要兼容旧结构
- 排查服务问题时，优先看：
  - `service.rs`
  - `shell.rs`
  - 实际 OpenClaw CLI 行为
- 排查安装问题时，优先看：
  - `installer.rs`
  - 各平台 shell 逻辑
  - 用户 PATH 假设
- 排查发布问题时，优先看：
  - `.github/workflows/release.yml`
  - `src-tauri/tauri.conf.json`
  - signing secrets
  - updater endpoint 和 public key 是否匹配当前发布仓库

## 建议验证顺序

如果后续要继续开发，建议按这个顺序验证：

1. `npm ci`
2. `npm run build`
3. 安装 Rust/Cargo
4. `cargo check --manifest-path src-tauri/Cargo.toml`
5. `npm run tauri:dev`
6. 手工 smoke test：
   - Dashboard
   - Setup
   - AIConfig
   - Channels
   - MCP
   - Skills
   - Agents
   - Settings
   - Logs
7. 然后再测依赖环境的功能：
   - Node install
   - OpenClaw install
   - gateway start/stop
   - Ollama
   - channel login
   - Clawhub
   - mcporter
   - updater

## 简短结论

这个仓库本质上是一个基于 Tauri 的桌面控制台，用来管理外部安装的 OpenClaw CLI。
它真正复杂的地方不在 UI 样式，而在这些点：

- shell/PATH 行为
- 配置变更的正确性
- 跨平台进程控制
- 对持续演进中的 OpenClaw 配置结构的兼容
- release 签名与 updater 产物生成
