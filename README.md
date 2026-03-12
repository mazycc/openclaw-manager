# 🦞 OpenClaw Manager

**One-click installer & management GUI for [OpenClaw](https://github.com/miaoxworld/OpenClawInstaller)** — the open-source AI assistant framework.

Built with **Tauri 2.0 + React 18 + TypeScript + Rust** for native performance on every desktop platform.

![Platform](https://img.shields.io/badge/platform-macOS%20|%20Windows%20|%20Linux-blue)
![Tauri](https://img.shields.io/badge/Tauri-2.0-orange)
![React](https://img.shields.io/badge/React-18-61DAFB)
![Rust](https://img.shields.io/badge/Rust-1.70+-red)

---

## ✨ Features & Usage Guide

### 🚀 One-Click Setup Wizard
Skip the terminal entirely. The built-in setup wizard automatically detects your environment, installs Node.js and OpenClaw, and initializes everything — all from the GUI.

### 📊 Dashboard & Service Control
Real-time monitoring and full lifecycle management of the OpenClaw service.
- **Service Status:** Port, PID, memory usage, uptime.
- **Service Supervisor**: Automatically revives the gateway when it is restarted via Telegram command or recovers from unexpected failures.
- **Log Viewer**: Structured local application logs. Filter by warnings, errors, and easily export.
- **Web Control UI**: Direct chat interface with your agents (`http://localhost:{GATEWAY_PORT}`).

### 🤖 Comprehensive AI Configuration
Flexible multi-provider AI connection with seamless **Ollama** integration.

**Supported Providers:**
- **Google Gemini** (New! ✨): Gemini 3 Pro, Gemini 3 Flash
- **Anthropic**: Claude 3.5 Sonnet, Opus
- **OpenAI**: GPT-4o, GPT-4o-mini
- **DeepSeek**: DeepSeek V3 (Chat), DeepSeek R1 (Reasoner)
- **Local Models (Ollama)**: Auto-detect Ollama installation. Search, pull, and manage local models (e.g., `llama3`, `qwen3.5:9b`) directly from the GUI.
- **Custom Provider Profile**: Add any OpenAI or Anthropic API-compatible endpoints and set your specific models.

### ⚙️ Advanced Settings & Tuning
Granular configuration over your entire OpenClaw ecosystem directly via the GUI.

- **Compaction & Memory Optimization**: Map tokens before compaction triggers, manage context pruning, limit message retention, and map offline local embeddings using Ollama.
- **Subagent Global Defaults**: Manage complex agent nesting limits. Define the max spawn depth, max children per agent, and limit concurrent subagent processing.
- **Tools & Security Profiles**: Set strict guardrails across your instances (Messaging, Minimal, Coding, Full Access). 
- **Native PDF Support**: Configure strict limits specifying maximum token pages and payload size (MB) for attached complex document processing.
- **Inline File Attachments**: Enable/Disable subagents analyzing dropped standard session attachment drops and define the max byte threshold per session.
- **Browser Control & Web Search**: Empower agents to explore the web by integrating your own Brave Search API keys and customize the internal agent Browser window UI Chrome colors.
- **Network Customization**: Easily adjust the Gateway Port dynamically (e.g., standard `3000`) and the global debug Log Level (e.g., debug, info, warn).
- **Workspace Localization**: Configure local timezones and preferred time format (e.g., 12h AM/PM vs 24h).

### 📋 Configuration Management
Never lose an `.openclaw.json` or model setup profile again!
- Validated GUI configurations directly synced to your `.openclaw.json`.
- Provide schema validation right from the interface.
- Import, Export, Backup, and Restore your entire setup locally using JSON.

### 🧩 MCP Management
Full [Model Context Protocol](https://modelcontextprotocol.io/) server management with integrated **mcporter** support. Set up simple StdIo local commands or remote SSE hooks dynamically. Changes automatically sink to your local `~/.mcporter/mcporter.json`.

### 📚 Skills Management
Browse, install, and manage OpenClaw capabilities explicitly shipped via **ClawHub** (e.g., specialized coding, web development). 

### 📱 Message Channels
Connect OpenClaw to multiple omnichannel chat platforms.
**Supported Channels:** Telegram, Feishu, Discord, Slack, WhatsApp. Complete configurations requiring tokens, secret hashes, IDs, authorized groups/users, direct from the interface to be bound instantly to the Gateway.

### 🔄 OpenClaw Manager Self-Update
Get automatic Over-The-Air (OTA) updates right inside the app settings! When a new version is built, be notified, pull the latest, and securely relaunch with newly built features—no manual reinstalling required!

---

## 📁 Project Structure

```
openclaw-manager/
├── src-tauri/                 # Rust Backend
│   ├── src/
│   │   ├── main.rs            # Entry point
│   │   ├── commands/          # Backend logic (config, install, service, etc.)
│   │   ├── models/            # Data structures
│   │   └── utils/             # Helpers
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── src/                       # React Frontend
│   ├── components/            # UI Components (Dashboard, Settings, specific features)
│   ├── hooks/                 # Custom Hooks
│   ├── lib/                   # API bindings
│   ├── stores/                # State management (Zustand)
│   └── styles/                # Tailwind CSS
│
├── package.json
└── vite.config.ts
```

---

## 🛠️ Tech Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| Frontend | React 18 | UI framework |
| State | Zustand | Lightweight reactive state |
| Styling | TailwindCSS | Utility-first CSS |
| Animation | Framer Motion | Smooth transitions & micro-interactions |
| Backend | Rust | High-performance system operations |
| Desktop | Tauri 2.0 | Native cross-platform shell |

---

## 🚀 Quick Start (Development)

### Prerequisites

| Tool | Version | Download |
|------|---------|----------|
| **Node.js** | >= 18.0 | [nodejs.org](https://nodejs.org/) |
| **Rust** | >= 1.70 | [rustup.rs](https://rustup.rs/) |
| **pnpm** or npm | Latest | Comes with Node.js |

### Clone & Run

```bash
git clone https://github.com/MrFadiAi/openclaw-one-click-installer.git
cd openclaw-one-click-installer

npm install          # Install dependencies
npm run tauri:dev    # Launch in development mode (hot-reload)
```

> **Note:** First build compiles all Rust dependencies and takes **3–5 minutes**. Subsequent runs are much faster.

### Build Release

```bash
npm run tauri:build
```

### Build Windows Offline Installer Bundle

```bash
# Download/package offline assets and stage into src-tauri/resources/offline/windows
npm run offline:windows:prepare

# Build NSIS installer with bundled offline assets
npm run tauri:build:windows:offline
```

Output in `src-tauri/target/release/bundle/`:

| Platform | Formats |
|----------|---------|
| macOS | `.dmg`, `.app` |
| Windows | `.msi`, `.exe` |
| Linux | `.deb`, `.AppImage` |

---

## 🤝 Contributing

1. Fork the project
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

---

## 📄 License

MIT License — See [LICENSE](LICENSE) for details.

---

**Made with ❤️ by the OpenClaw Community**
