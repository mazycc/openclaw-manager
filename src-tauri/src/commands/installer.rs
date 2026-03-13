use crate::utils::{log_sanitizer, platform, shell};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{command, path::BaseDirectory, AppHandle, Manager};

const POWERSHELL_GIT_HTTPS_SETUP: &str = r#"
$gitConfig = Join-Path $env:TEMP 'openclaw-manager-git-install.conf'
Set-Content -Path $gitConfig -Value '' -Encoding Ascii
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:GIT_CONFIG_COUNT = '2'
$env:GIT_CONFIG_KEY_0 = 'url.https://github.com/.insteadOf'
$env:GIT_CONFIG_VALUE_0 = 'ssh://git@github.com/'
$env:GIT_CONFIG_KEY_1 = 'url.https://github.com/.insteadOf'
$env:GIT_CONFIG_VALUE_1 = 'git@github.com:'
$env:GIT_TERMINAL_PROMPT = '0'
$env:PATH = "$env:APPDATA\npm;$env:PATH"
"#;

const BASH_GIT_HTTPS_SETUP: &str = r#"
GIT_CONFIG_GLOBAL="${TMPDIR:-/tmp}/openclaw-manager-git-install.conf"
: > "$GIT_CONFIG_GLOBAL"
export GIT_CONFIG_GLOBAL
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_COUNT=2
export GIT_CONFIG_KEY_0='url.https://github.com/.insteadOf'
export GIT_CONFIG_VALUE_0='ssh://git@github.com/'
export GIT_CONFIG_KEY_1='url.https://github.com/.insteadOf'
export GIT_CONFIG_VALUE_1='git@github.com:'
export GIT_TERMINAL_PROMPT=0
NPM_PREFIX="$(npm prefix -g 2>/dev/null || true)"
if [ -n "$NPM_PREFIX" ]; then
    export PATH="$NPM_PREFIX/bin:$NPM_PREFIX:$PATH"
fi
"#;

const POWERSHELL_FIND_OPENCLAW: &str = r#"
$openclawCmd = $null
$npmPrefix = npm prefix -g 2>$null
if ($npmPrefix) {
    $candidate = Join-Path $npmPrefix 'openclaw.cmd'
    if (Test-Path $candidate) {
        $openclawCmd = $candidate
    }
}
if (-not $openclawCmd -and $env:APPDATA) {
    $candidate = Join-Path $env:APPDATA 'npm\openclaw.cmd'
    if (Test-Path $candidate) {
        $openclawCmd = $candidate
    }
}
if (-not $openclawCmd -and $env:LOCALAPPDATA) {
    $candidate = Join-Path $env:LOCALAPPDATA 'Programs\OpenClaw\npm-global\openclaw.cmd'
    if (Test-Path $candidate) {
        $openclawCmd = $candidate
    }
}
if (-not $openclawCmd) {
    $command = Get-Command openclaw -ErrorAction SilentlyContinue
    if ($command) {
        $openclawCmd = $command.Source
    }
}
if (-not $openclawCmd) {
    throw 'OpenClaw CLI was installed but could not be found in the npm global directory or PATH.'
}
"#;

const BASH_FIND_OPENCLAW: &str = r#"
OPENCLAW_BIN=''
NPM_PREFIX="$(npm prefix -g 2>/dev/null || true)"
if [ -n "$NPM_PREFIX" ]; then
    if [ -x "$NPM_PREFIX/bin/openclaw" ]; then
        OPENCLAW_BIN="$NPM_PREFIX/bin/openclaw"
    elif [ -x "$NPM_PREFIX/openclaw" ]; then
        OPENCLAW_BIN="$NPM_PREFIX/openclaw"
    fi
fi
if [ -z "$OPENCLAW_BIN" ]; then
    OPENCLAW_BIN="$(command -v openclaw 2>/dev/null || true)"
fi
if [ -z "$OPENCLAW_BIN" ]; then
    echo "OpenClaw CLI was installed but could not be found in the npm global directory or PATH." >&2
    exit 1
fi
"#;

// Bundled resource root for offline Windows installers:
//   node/*.msi|*.zip, git/*.exe, openclaw/*.tgz
const WINDOWS_OFFLINE_ASSETS_SUBDIR: &str = "offline/windows";

fn powershell_single_quoted_path(path: &Path) -> String {
    // Escape single quotes so path can be used in a PowerShell single-quoted string literal.
    path.to_string_lossy().replace('\'', "''")
}

fn resolve_windows_offline_assets_root(app: &AppHandle) -> Option<PathBuf> {
    if !platform::is_windows() {
        return None;
    }

    if let Ok(custom_path) = std::env::var("OPENCLAW_MANAGER_OFFLINE_ASSETS_DIR") {
        let candidate = PathBuf::from(custom_path);
        if candidate.exists() {
            info!(
                "[Offline Assets] Using custom Windows offline assets path: {}",
                candidate.display()
            );
            return Some(candidate);
        }
        warn!(
            "[Offline Assets] OPENCLAW_MANAGER_OFFLINE_ASSETS_DIR is set but path does not exist: {}",
            candidate.display()
        );
    }

    if let Ok(resource_path) = app
        .path()
        .resolve(WINDOWS_OFFLINE_ASSETS_SUBDIR, BaseDirectory::Resource)
    {
        if resource_path.exists() {
            info!(
                "[Offline Assets] Using bundled Windows offline assets path: {}",
                resource_path.display()
            );
            return Some(resource_path);
        }
    }

    // Dev fallback: allow local testing before packaging.
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("offline")
        .join("windows");
    if dev_path.exists() {
        info!(
            "[Offline Assets] Using local dev Windows offline assets path: {}",
            dev_path.display()
        );
        return Some(dev_path);
    }

    info!("[Offline Assets] No Windows offline assets directory found, using online installation flow");
    None
}

/// Environment check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentStatus {
    /// Whether Node.js is installed
    pub node_installed: bool,
    /// Node.js version
    pub node_version: Option<String>,
    /// Whether Node.js version meets requirement (>=22)
    pub node_version_ok: bool,
    /// Whether Git is installed
    pub git_installed: bool,
    /// Git version
    pub git_version: Option<String>,
    /// Whether OpenClaw is installed
    pub openclaw_installed: bool,
    /// OpenClaw version
    pub openclaw_version: Option<String>,
    /// Whether gateway service is installed
    pub gateway_service_installed: bool,
    /// Whether config directory exists
    pub config_dir_exists: bool,
    /// Whether everything is ready
    pub ready: bool,
    /// Operating system
    pub os: String,
}

/// Installation progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    pub step: String,
    pub progress: u8,
    pub message: String,
    pub error: Option<String>,
}

/// Installation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub success: bool,
    pub message: String,
    pub error: Option<String>,
}

/// Check environment status
#[command]
pub async fn check_environment() -> Result<EnvironmentStatus, String> {
    info!("[Environment Check] Starting system environment check...");

    let os = platform::get_os();
    info!("[Environment Check] Operating system: {}", os);

    // Run expensive checks concurrently
    info!("[Environment Check] Checking Node.js, Git, and OpenClaw concurrently...");
    let (node_res, git_res, openclaw_res) = tokio::join!(
        tokio::task::spawn_blocking(|| get_node_version()),
        tokio::task::spawn_blocking(|| get_git_version()),
        tokio::task::spawn_blocking(|| get_openclaw_version())
    );

    let node_version = node_res.unwrap_or(None);
    let git_version = git_res.unwrap_or(None);
    let openclaw_version = openclaw_res.unwrap_or(None);

    let node_installed = node_version.is_some();
    let node_version_ok = check_node_version_requirement(&node_version);
    info!(
        "[Environment Check] Node.js: installed={}, version={:?}, version_ok={}",
        node_installed, node_version, node_version_ok
    );

    let git_installed = git_version.is_some();
    info!(
        "[Environment Check] Git: installed={}, version={:?}",
        git_installed, git_version
    );

    let openclaw_installed = openclaw_version.is_some();
    info!(
        "[Environment Check] OpenClaw: installed={}, version={:?}",
        openclaw_installed, openclaw_version
    );

    // Check Gateway Service (only if OpenClaw is installed)
    let gateway_service_installed = if openclaw_installed {
        info!("[Environment Check] Checking Gateway Service...");
        let installed = tokio::task::spawn_blocking(|| check_gateway_installed())
            .await
            .unwrap_or(false);
        info!(
            "[Environment Check] Gateway Service: installed={}",
            installed
        );
        installed
    } else {
        false
    };

    // Check config directory
    let config_dir = platform::get_config_dir();
    let config_dir_exists = std::path::Path::new(&config_dir).exists();
    info!(
        "[Environment Check] Config directory: {}, exists={}",
        config_dir, config_dir_exists
    );

    let ready =
        node_installed && node_version_ok && openclaw_installed && gateway_service_installed;
    info!(
        "[Environment Check] Environment ready status: ready={}",
        ready
    );

    Ok(EnvironmentStatus {
        node_installed,
        node_version,
        node_version_ok,
        git_installed,
        git_version,
        openclaw_installed,
        openclaw_version,
        gateway_service_installed,
        config_dir_exists,
        ready,
        os,
    })
}

/// Get Node.js version
/// Detects multiple possible installation paths, since GUI apps don't inherit user shell PATH
fn get_node_version() -> Option<String> {
    if platform::is_windows() {
        // Windows: First try direct call (if PATH is updated)
        if let Ok(v) = shell::run_cmd_output("node --version") {
            let version = v.trim().to_string();
            if !version.is_empty() && version.starts_with('v') {
                info!("[Environment Check] Found Node.js via PATH: {}", version);
                return Some(version);
            }
        }

        // Windows: Check common installation paths
        let possible_paths = get_windows_node_paths();
        for path in possible_paths {
            if std::path::Path::new(&path).exists() {
                // Execute using full path
                let cmd = format!("\"{}\" --version", path);
                if let Ok(output) = shell::run_cmd_output(&cmd) {
                    let version = output.trim().to_string();
                    if !version.is_empty() && version.starts_with('v') {
                        info!("[Environment Check] Found Node.js at {}: {}", path, version);
                        return Some(version);
                    }
                }
            }
        }

        None
    } else {
        // First try direct call
        if let Ok(v) = shell::run_command_output("node", &["--version"]) {
            return Some(v.trim().to_string());
        }

        // Detect common Node.js installation paths (macOS/Linux)
        let possible_paths = get_unix_node_paths();
        for path in possible_paths {
            if std::path::Path::new(&path).exists() {
                if let Ok(output) = shell::run_command_output(&path, &["--version"]) {
                    info!(
                        "[Environment Check] Found Node.js at {}: {}",
                        path,
                        output.trim()
                    );
                    return Some(output.trim().to_string());
                }
            }
        }

        // Try to detect by loading user environment via shell
        if let Ok(output) = shell::run_bash_output("source ~/.zshrc 2>/dev/null || source ~/.bashrc 2>/dev/null; node --version 2>/dev/null") {
            if !output.is_empty() && output.starts_with('v') {
                info!("[Environment Check] Found Node.js via user shell: {}", output.trim());
                return Some(output.trim().to_string());
            }
        }

        None
    }
}

/// Get possible Node.js paths on Unix systems
fn get_unix_node_paths() -> Vec<String> {
    let mut paths = Vec::new();

    // Homebrew (macOS)
    paths.push("/opt/homebrew/bin/node".to_string()); // Apple Silicon
    paths.push("/usr/local/bin/node".to_string()); // Intel Mac

    // System installation
    paths.push("/usr/bin/node".to_string());

    // nvm (check common versions)
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();

        // nvm default versions
        paths.push(format!("{}/.nvm/versions/node/v22.0.0/bin/node", home_str));
        paths.push(format!("{}/.nvm/versions/node/v22.1.0/bin/node", home_str));
        paths.push(format!("{}/.nvm/versions/node/v22.2.0/bin/node", home_str));
        paths.push(format!("{}/.nvm/versions/node/v22.11.0/bin/node", home_str));
        paths.push(format!("{}/.nvm/versions/node/v22.12.0/bin/node", home_str));
        paths.push(format!("{}/.nvm/versions/node/v23.0.0/bin/node", home_str));

        // Try nvm alias default (read nvm's default alias)
        let nvm_default = format!("{}/.nvm/alias/default", home_str);
        if let Ok(version) = std::fs::read_to_string(&nvm_default) {
            let version = version.trim();
            if !version.is_empty() {
                paths.insert(
                    0,
                    format!("{}/.nvm/versions/node/v{}/bin/node", home_str, version),
                );
            }
        }

        // fnm
        paths.push(format!("{}/.fnm/aliases/default/bin/node", home_str));

        // volta
        paths.push(format!("{}/.volta/bin/node", home_str));

        // asdf
        paths.push(format!("{}/.asdf/shims/node", home_str));

        // mise (formerly rtx)
        paths.push(format!("{}/.local/share/mise/shims/node", home_str));
    }

    paths
}

/// Get possible Node.js paths on Windows systems
fn get_windows_node_paths() -> Vec<String> {
    let mut paths = Vec::new();

    // 1. Standard installation paths (Program Files)
    paths.push("C:\\Program Files\\nodejs\\node.exe".to_string());
    paths.push("C:\\Program Files (x86)\\nodejs\\node.exe".to_string());

    // 2. nvm for Windows (nvm4w) - common installation location
    paths.push("C:\\nvm4w\\nodejs\\node.exe".to_string());

    // 3. Various installations in user directory
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();

        // nvm for Windows user installation
        paths.push(format!(
            "{}\\AppData\\Roaming\\nvm\\current\\node.exe",
            home_str
        ));

        // fnm (Fast Node Manager) for Windows
        paths.push(format!(
            "{}\\AppData\\Roaming\\fnm\\aliases\\default\\node.exe",
            home_str
        ));
        paths.push(format!(
            "{}\\AppData\\Local\\fnm\\aliases\\default\\node.exe",
            home_str
        ));
        paths.push(format!("{}\\.fnm\\aliases\\default\\node.exe", home_str));

        // volta
        paths.push(format!(
            "{}\\AppData\\Local\\Volta\\bin\\node.exe",
            home_str
        ));
        // volta invokes via shim, just check bin directory

        // scoop installation
        paths.push(format!(
            "{}\\scoop\\apps\\nodejs\\current\\node.exe",
            home_str
        ));
        paths.push(format!(
            "{}\\scoop\\apps\\nodejs-lts\\current\\node.exe",
            home_str
        ));

        // OpenClaw portable fallback installation path
        paths.push(format!(
            "{}\\AppData\\Local\\Programs\\OpenClaw\\nodejs\\node.exe",
            home_str
        ));

        // chocolatey installation
        paths.push("C:\\ProgramData\\chocolatey\\lib\\nodejs\\tools\\node.exe".to_string());
    }

    // 4. Installation paths from registry (obtained indirectly via environment variables)
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        paths.push(format!("{}\\nodejs\\node.exe", program_files));
    }
    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        paths.push(format!("{}\\nodejs\\node.exe", program_files_x86));
    }

    // 5. nvm-windows symlink path (NVM_SYMLINK environment variable)
    if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
        paths.insert(0, format!("{}\\node.exe", nvm_symlink));
    }

    // 6. Current version under nvm-windows NVM_HOME path
    if let Ok(nvm_home) = std::env::var("NVM_HOME") {
        // Try to read the currently activated version
        let settings_path = format!("{}\\settings.txt", nvm_home);
        if let Ok(content) = std::fs::read_to_string(&settings_path) {
            for line in content.lines() {
                if line.starts_with("current:") {
                    if let Some(version) = line.strip_prefix("current:") {
                        let version = version.trim();
                        if !version.is_empty() {
                            paths.insert(0, format!("{}\\v{}\\node.exe", nvm_home, version));
                        }
                    }
                }
            }
        }
    }

    paths
}

/// Get Git version
fn get_git_version() -> Option<String> {
    if platform::is_windows() {
        if let Ok(v) = shell::run_cmd_output("git --version") {
            let version = v.trim().to_string();
            if !version.is_empty() && version.contains("git version") {
                // "git version 2.43.0.windows.1" -> "2.43.0"
                let ver = version.replace("git version ", "");
                let ver = ver.split('.').take(3).collect::<Vec<_>>().join(".");
                return Some(ver);
            }
        }
        None
    } else {
        if let Ok(v) = shell::run_command_output("git", &["--version"]) {
            let version = v.trim().to_string();
            if !version.is_empty() && version.contains("git version") {
                let ver = version.replace("git version ", "");
                return Some(ver.trim().to_string());
            }
        }
        None
    }
}

/// Get OpenClaw version
fn get_openclaw_version() -> Option<String> {
    // Use run_openclaw to handle all platforms uniformly
    shell::run_openclaw(&["--version"])
        .ok()
        .map(|v| v.trim().to_string())
}

/// Check if Node.js version is >= 22
fn check_node_version_requirement(version: &Option<String>) -> bool {
    if let Some(v) = version {
        // Parse version "v22.1.0" -> 22
        let major = v
            .trim_start_matches('v')
            .split('.')
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        major >= 22
    } else {
        false
    }
}

/// Check if gateway service is installed
fn check_gateway_installed() -> bool {
    match shell::run_openclaw(&["gateway", "status"]) {
        Ok(output) => {
            let lower = output.to_lowercase();
            // If output contains "not installed" or "not found", it's not installed
            if lower.contains("not installed") || lower.contains("not found") {
                return false;
            }
            // If the command succeeded, consider it installed
            true
        }
        Err(e) => {
            let lower = e.to_lowercase();
            // Some versions return error when not installed
            if lower.contains("not installed") || lower.contains("not found") {
                return false;
            }
            // If the command itself failed (e.g. openclaw not found), not installed
            debug!("[Environment Check] Gateway status check failed: {}", e);
            false
        }
    }
}

/// Install gateway service (opens elevated terminal)
#[command]
pub async fn install_gateway_service() -> Result<String, String> {
    info!("[Gateway Install] Starting gateway service installation...");
    let os = platform::get_os();
    info!("[Gateway Install] Detected operating system: {}", os);

    match os.as_str() {
        "windows" => install_gateway_windows().await,
        "macos" => install_gateway_macos().await,
        "linux" => install_gateway_linux().await,
        _ => Err(format!("Unsupported operating system: {}", os)),
    }
}

/// Install gateway service on Windows (elevated PowerShell)
async fn install_gateway_windows() -> Result<String, String> {
    info!("[Gateway Install] Opening elevated PowerShell for gateway install...");

    // Find openclaw path to use in the script
    let openclaw_path = shell::get_openclaw_path().unwrap_or_else(|| "openclaw".to_string());
    let escaped_path = openclaw_path.replace('\\', "\\\\");

    let script = format!(
        r#"
Start-Process powershell -ArgumentList '-NoExit', '-Command', '
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  OpenClaw Gateway Service Installer" -ForegroundColor White
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Installing OpenClaw Gateway as a system service..." -ForegroundColor Yellow
Write-Host ""

try {{
    & "{}" gateway install
    Write-Host ""
    Write-Host "Gateway service installed successfully!" -ForegroundColor Green
}} catch {{
    Write-Host "Installation failed: $_" -ForegroundColor Red
}}

Write-Host ""
Write-Host "You can close this window and click Refresh in OpenClaw小白安装工具." -ForegroundColor Cyan
Write-Host ""
Read-Host "Press Enter to close this window"
' -Verb RunAs
"#,
        escaped_path
    );

    match shell::run_powershell_output(&script) {
        Ok(_) => {
            info!("[Gateway Install] Elevated terminal launched successfully");
            Ok("Gateway install terminal opened with administrator privileges. Please complete the installation and click Refresh.".to_string())
        }
        Err(e) => {
            warn!(
                "[Gateway Install] Failed to launch elevated terminal: {}",
                e
            );
            Err(format!("Failed to open administrator terminal: {}. Please open PowerShell as Administrator and run: openclaw gateway install", e))
        }
    }
}

/// Install gateway service on macOS (Terminal with sudo)
async fn install_gateway_macos() -> Result<String, String> {
    info!("[Gateway Install] Opening terminal for gateway install on macOS...");

    let script_content = r#"#!/bin/bash
clear
echo "========================================"
echo "  OpenClaw Gateway Service Installer"
echo "========================================"
echo ""
echo "Installing OpenClaw Gateway as a system service..."
echo "You may be prompted for your password."
echo ""

sudo openclaw gateway install

echo ""
if [ $? -eq 0 ]; then
    echo "✅ Gateway service installed successfully!"
else
    echo "❌ Installation failed. Please check the error above."
fi
echo ""
echo "You can close this window and click Refresh in OpenClaw小白安装工具."
read -p "Press Enter to close this window..."
"#;

    let script_path = "/tmp/openclaw_gateway_install.command";
    std::fs::write(script_path, script_content)
        .map_err(|e| format!("Failed to create script: {}", e))?;

    std::process::Command::new("chmod")
        .args(["+x", script_path])
        .output()
        .map_err(|e| format!("Failed to set permissions: {}", e))?;

    std::process::Command::new("open")
        .arg(script_path)
        .spawn()
        .map_err(|e| format!("Failed to launch terminal: {}", e))?;

    info!("[Gateway Install] Terminal launched successfully on macOS");
    Ok("Gateway install terminal opened. Please enter your password when prompted and click Refresh after completion.".to_string())
}

/// Install gateway service on Linux (terminal with sudo)
async fn install_gateway_linux() -> Result<String, String> {
    info!("[Gateway Install] Opening terminal for gateway install on Linux...");

    let script_content = r#"#!/bin/bash
clear
echo "========================================"
echo "  OpenClaw Gateway Service Installer"
echo "========================================"
echo ""
echo "Installing OpenClaw Gateway as a system service..."
echo "You may be prompted for your password."
echo ""

sudo openclaw gateway install

echo ""
if [ $? -eq 0 ]; then
    echo "✅ Gateway service installed successfully!"
else
    echo "❌ Installation failed. Please check the error above."
fi
echo ""
echo "You can close this window and click Refresh in OpenClaw小白安装工具."
read -p "Press Enter to close this window..."
"#;

    let script_path = "/tmp/openclaw_gateway_install.sh";
    std::fs::write(script_path, script_content)
        .map_err(|e| format!("Failed to create script: {}", e))?;

    std::process::Command::new("chmod")
        .args(["+x", script_path])
        .output()
        .map_err(|e| format!("Failed to set permissions: {}", e))?;

    // Try different terminal emulators
    let terminals = ["gnome-terminal", "xfce4-terminal", "konsole", "xterm"];
    for term in terminals {
        if std::process::Command::new(term)
            .args(["--", script_path])
            .spawn()
            .is_ok()
        {
            info!(
                "[Gateway Install] Terminal '{}' launched successfully on Linux",
                term
            );
            return Ok("Gateway install terminal opened. Please enter your password when prompted and click Refresh after completion.".to_string());
        }
    }

    warn!("[Gateway Install] No terminal emulator found on Linux");
    Err(
        "Unable to launch terminal. Please open a terminal and run: sudo openclaw gateway install"
            .to_string(),
    )
}

/// Install Node.js
#[command]
pub async fn install_nodejs(app: AppHandle) -> Result<InstallResult, String> {
    info!("[Install Node.js] Starting Node.js installation...");
    let os = platform::get_os();
    info!("[Install Node.js] Detected operating system: {}", os);
    let windows_offline_assets_root = resolve_windows_offline_assets_root(&app);

    let result = match os.as_str() {
        "windows" => {
            info!("[Install Node.js] Using Windows installation method...");
            install_nodejs_windows(windows_offline_assets_root).await
        }
        "macos" => {
            info!("[Install Node.js] Using macOS installation method (Homebrew)...");
            install_nodejs_macos().await
        }
        "linux" => {
            info!("[Install Node.js] Using Linux installation method...");
            install_nodejs_linux().await
        }
        _ => {
            error!("[Install Node.js] Unsupported operating system: {}", os);
            Ok(InstallResult {
                success: false,
                message: "Unsupported operating system".to_string(),
                error: Some(format!("Unsupported operating system: {}", os)),
            })
        }
    };

    match &result {
        Ok(r) if r.success => info!("[Install Node.js] Installation successful"),
        Ok(r) => warn!("[Install Node.js] Installation failed: {}", r.message),
        Err(e) => error!("[Install Node.js] Installation error: {}", e),
    }

    result
}

/// Install Node.js on Windows
async fn install_nodejs_windows(windows_offline_assets_root: Option<PathBuf>) -> Result<InstallResult, String> {
    let offline_assets_root = windows_offline_assets_root
        .as_deref()
        .map(powershell_single_quoted_path)
        .unwrap_or_default();
    let script = r#"
$ErrorActionPreference = 'Stop'
$offlineAssetsRoot = '__OPENCLAW_OFFLINE_ASSETS_ROOT__'

function Test-NodeInstalled {
    $nodeVersion = node --version 2>$null
    if ($nodeVersion) {
        Write-Host "Node.js is already installed: $nodeVersion"
        return $true
    }
    return $false
}

function Get-NodeArchToken {
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {
        return "arm64"
    }
    if ([Environment]::Is64BitOperatingSystem) {
        return "x64"
    }
    return "x86"
}

function Get-OfflineArchRegex {
    $arch = Get-NodeArchToken
    if ($arch -eq "arm64") {
        return "(arm64|aarch64)"
    }
    if ($arch -eq "x86") {
        return "(x86|32-bit)"
    }
    return "(x64|amd64|64-bit)"
}

function Resolve-OfflineNodeAsset {
    param(
        [Parameter(Mandatory = $true)][string]$Extension
    )

    if ([string]::IsNullOrWhiteSpace($offlineAssetsRoot)) {
        return $null
    }

    $nodeDir = Join-Path $offlineAssetsRoot "node"
    if (-not (Test-Path $nodeDir)) {
        return $null
    }

    $regex = Get-OfflineArchRegex
    $archMatch = Get-ChildItem -Path $nodeDir -File -Filter "*.$Extension" -ErrorAction SilentlyContinue |
        Where-Object { $_.Name.ToLowerInvariant() -match $regex } |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($archMatch) {
        return $archMatch.FullName
    }

    $fallback = Get-ChildItem -Path $nodeDir -File -Filter "*.$Extension" -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($fallback) {
        return $fallback.FullName
    }

    return $null
}

function Install-NodeViaLocalMsi {
    param([string]$msiPath)

    try {
        Write-Host "Installing Node.js from bundled MSI: $msiPath"
        & msiexec.exe /i $msiPath /qn /norestart
        $msiExitCode = $LASTEXITCODE
        if ($msiExitCode -eq 0 -or $msiExitCode -eq 3010) {
            return $true
        }
        Write-Host "Bundled MSI installation failed (exit code: $msiExitCode)"
        return $false
    } catch {
        Write-Host "Bundled MSI installation failed: $($_.Exception.Message)"
        return $false
    }
}

function Resolve-LatestNode22Version {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $index = Invoke-RestMethod -Uri "https://nodejs.org/dist/index.json" -UseBasicParsing
    $latest22 = $index | Where-Object { $_.version -like "v22.*" } | Select-Object -First 1
    if (-not $latest22) {
        throw "Unable to resolve latest Node.js v22 release from nodejs.org"
    }
    return $latest22.version.TrimStart("v")
}

function Install-NodeViaWinget {
    $hasWinget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $hasWinget) {
        return $false
    }

    Write-Host "Installing Node.js using winget (source: winget)..."
    $wingetArgs = @(
        "install",
        "--id", "OpenJS.NodeJS.LTS",
        "--source", "winget",
        "--exact",
        "--accept-source-agreements",
        "--accept-package-agreements"
    )

    winget @wingetArgs
    if ($LASTEXITCODE -eq 0) {
        return $true
    }

    Write-Host "winget install failed (exit code: $LASTEXITCODE). Refreshing sources and retrying..."
    winget source update winget
    if ($LASTEXITCODE -ne 0) {
        winget source reset --force
    }
    winget @wingetArgs
    return ($LASTEXITCODE -eq 0)
}

function Install-NodeViaMsi {
    try {
        $version = Resolve-LatestNode22Version
        $arch = Get-NodeArchToken
        $msiUrl = "https://nodejs.org/dist/v$version/node-v$version-$arch.msi"
        $msiPath = Join-Path $env:TEMP "node-v$version-$arch.msi"

        Write-Host "Installing Node.js using official MSI fallback..."
        Write-Host "MSI URL: $msiUrl"
        Invoke-WebRequest -Uri $msiUrl -OutFile $msiPath -UseBasicParsing

        & msiexec.exe /i $msiPath /qn /norestart
        $msiExitCode = $LASTEXITCODE
        if ($msiExitCode -eq 0 -or $msiExitCode -eq 3010) {
            return $true
        }

        Write-Host "MSI installation failed (exit code: $msiExitCode)"
        return $false
    } catch {
        Write-Host "MSI fallback failed: $($_.Exception.Message)"
        return $false
    }
}

function Install-NodePortable {
    param([string]$preDownloadedZip = "")

    try {
        $arch = Get-NodeArchToken
        $extractRoot = Join-Path $env:TEMP ("openclaw-node-install-" + [Guid]::NewGuid().ToString("N"))
        $targetRoot = Join-Path $env:LOCALAPPDATA "Programs\OpenClaw\nodejs"

        if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
            throw "LOCALAPPDATA is unavailable, cannot install Node.js portable fallback"
        }

        if ($preDownloadedZip -and (Test-Path $preDownloadedZip)) {
            $zipPath = $preDownloadedZip
            Write-Host "Installing Node.js from bundled ZIP: $zipPath"
        } else {
            $version = Resolve-LatestNode22Version
            $zipUrl = "https://nodejs.org/dist/v$version/node-v$version-win-$arch.zip"
            $zipPath = Join-Path $env:TEMP "node-v$version-win-$arch.zip"
            Write-Host "Installing Node.js portable fallback to user profile..."
            Write-Host "ZIP URL: $zipUrl"
            Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath -UseBasicParsing
        }

        if (Test-Path $extractRoot) {
            Remove-Item $extractRoot -Recurse -Force
        }
        Expand-Archive -Path $zipPath -DestinationPath $extractRoot -Force

        $expandedDir = Get-ChildItem -Path $extractRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { Test-Path (Join-Path $_.FullName "node.exe") } |
            Select-Object -First 1
        if (-not $expandedDir) {
            throw "Portable package unpack failed: node.exe not found after extraction"
        }

        if (Test-Path $targetRoot) {
            Remove-Item $targetRoot -Recurse -Force
        }
        New-Item -ItemType Directory -Path $targetRoot -Force | Out-Null
        Copy-Item -Path (Join-Path $expandedDir.FullName "*") -Destination $targetRoot -Recurse -Force

        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if (-not $userPath) { $userPath = "" }
        $parts = $userPath -split ";" | Where-Object { $_ -and $_.Trim() -ne "" }
        if (-not ($parts -contains $targetRoot)) {
            $newPath = if ($parts.Count -gt 0) { ($parts + $targetRoot) -join ";" } else { $targetRoot }
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        }

        $env:Path = "$targetRoot;$env:Path"
        return $true
    } catch {
        Write-Host "Portable fallback failed: $($_.Exception.Message)"
        return $false
    }
}

function Install-NodeFromOfflineAssets {
    if ([string]::IsNullOrWhiteSpace($offlineAssetsRoot) -or -not (Test-Path $offlineAssetsRoot)) {
        return $false
    }

    Write-Host "Trying bundled offline Node.js assets from: $offlineAssetsRoot"

    $offlineMsi = Resolve-OfflineNodeAsset -Extension "msi"
    if ($offlineMsi) {
        if (Install-NodeViaLocalMsi -msiPath $offlineMsi) {
            return $true
        }
    }

    $offlineZip = Resolve-OfflineNodeAsset -Extension "zip"
    if ($offlineZip) {
        if (Install-NodePortable -preDownloadedZip $offlineZip) {
            return $true
        }
    }

    return $false
}

function Install-NodeViaFnm {
    try {
        Write-Host "Attempting to install Node.js using fnm..."
        $fnmInstallScript = "irm https://fnm.vercel.app/install.ps1 | iex"
        Invoke-Expression $fnmInstallScript

        $env:FNM_DIR = "$env:USERPROFILE\.fnm"
        $env:Path = "$env:FNM_DIR;$env:Path"

        fnm install 22
        fnm default 22
        fnm use 22

        return $true
    } catch {
        Write-Host "fnm fallback failed: $($_.Exception.Message)"
        return $false
    }
}

if (Test-NodeInstalled) { exit 0 }

if (Install-NodeFromOfflineAssets -and (Test-NodeInstalled)) {
    Write-Host "Node.js installed successfully via bundled offline assets"
    exit 0
}

if (Install-NodeViaWinget -and (Test-NodeInstalled)) {
    Write-Host "Node.js installed successfully via winget"
    exit 0
}

if (Install-NodeViaFnm -and (Test-NodeInstalled)) {
    Write-Host "Node.js installed successfully via fnm"
    exit 0
}

if (Install-NodeViaMsi -and (Test-NodeInstalled)) {
    Write-Host "Node.js installed successfully via MSI fallback"
    exit 0
}

if (Install-NodePortable -and (Test-NodeInstalled)) {
    Write-Host "Node.js installed successfully via portable fallback"
    exit 0
}

Write-Host "Node.js installation failed after all fallback methods."
exit 1
"#
    .replace("__OPENCLAW_OFFLINE_ASSETS_ROOT__", &offline_assets_root);

    match shell::run_powershell_output(&script) {
        Ok(output) => {
            // Verify installation
            if get_node_version().is_some() {
                Ok(InstallResult {
                    success: true,
                    message: "Node.js installed successfully! Please restart the application for environment variables to take effect.".to_string(),
                    error: None,
                })
            } else {
                Ok(InstallResult {
                    success: false,
                    message: "Application restart required after installation".to_string(),
                    error: Some(output),
                })
            }
        }
        Err(e) => Ok(InstallResult {
            success: false,
            message: "Node.js installation failed".to_string(),
            error: Some(e),
        }),
    }
}

/// Install Node.js on macOS
async fn install_nodejs_macos() -> Result<InstallResult, String> {
    // Install using Homebrew
    let script = r#"
# Check Homebrew
if ! command -v brew &> /dev/null; then
    echo "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

    # Configure PATH
    if [[ -f /opt/homebrew/bin/brew ]]; then
        eval "$(/opt/homebrew/bin/brew shellenv)"
    elif [[ -f /usr/local/bin/brew ]]; then
        eval "$(/usr/local/bin/brew shellenv)"
    fi
fi

echo "Installing Node.js 22..."
brew install node@22
brew link --overwrite node@22

# Verify installation
node --version
"#;

    match shell::run_bash_output(script) {
        Ok(output) => Ok(InstallResult {
            success: true,
            message: format!("Node.js installed successfully! {}", output),
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            success: false,
            message: "Node.js installation failed".to_string(),
            error: Some(e),
        }),
    }
}

/// Install Node.js on Linux
async fn install_nodejs_linux() -> Result<InstallResult, String> {
    // Install using NodeSource repository
    let script = r#"
# Detect package manager
if command -v apt-get &> /dev/null; then
    echo "Detected apt, using NodeSource repository..."
    curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
    sudo apt-get install -y nodejs
elif command -v dnf &> /dev/null; then
    echo "Detected dnf, using NodeSource repository..."
    curl -fsSL https://rpm.nodesource.com/setup_22.x | sudo bash -
    sudo dnf install -y nodejs
elif command -v yum &> /dev/null; then
    echo "Detected yum, using NodeSource repository..."
    curl -fsSL https://rpm.nodesource.com/setup_22.x | sudo bash -
    sudo yum install -y nodejs
elif command -v pacman &> /dev/null; then
    echo "Detected pacman..."
    sudo pacman -S nodejs npm --noconfirm
else
    echo "Unable to detect a supported package manager"
    exit 1
fi

# Verify installation
node --version
"#;

    match shell::run_bash_output(script) {
        Ok(output) => Ok(InstallResult {
            success: true,
            message: format!("Node.js installed successfully! {}", output),
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            success: false,
            message: "Node.js installation failed".to_string(),
            error: Some(e),
        }),
    }
}

/// Install OpenClaw
#[command]
pub async fn install_openclaw(app: AppHandle) -> Result<InstallResult, String> {
    info!("[Install OpenClaw] Starting OpenClaw installation...");
    let os = platform::get_os();
    info!("[Install OpenClaw] Detected operating system: {}", os);
    let windows_offline_assets_root = resolve_windows_offline_assets_root(&app);

    let result = match os.as_str() {
        "windows" => {
            info!("[Install OpenClaw] Using Windows installation method...");
            install_openclaw_windows(windows_offline_assets_root).await
        }
        _ => {
            info!("[Install OpenClaw] Using Unix installation method (npm)...");
            install_openclaw_unix().await
        }
    };

    match &result {
        Ok(r) if r.success => info!("[Install OpenClaw] Installation successful"),
        Ok(r) => warn!("[Install OpenClaw] Installation failed: {}", r.message),
        Err(e) => error!("[Install OpenClaw] Installation error: {}", e),
    }

    result
}

/// Install OpenClaw on Windows
async fn install_openclaw_windows(
    windows_offline_assets_root: Option<PathBuf>,
) -> Result<InstallResult, String> {
    let offline_assets_root = windows_offline_assets_root
        .as_deref()
        .map(powershell_single_quoted_path)
        .unwrap_or_default();
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
{git_setup}
$offlineAssetsRoot = '{offline_assets_root}'

# Check Node.js
$nodeVersion = node --version 2>$null
if (-not $nodeVersion) {{
    throw "Please install Node.js first"
}}

function Get-OfflineArchRegex {{
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {{
        return "(arm64|aarch64)"
    }}
    if ([Environment]::Is64BitOperatingSystem) {{
        return "(x64|amd64|64-bit)"
    }}
    return "(x86|32-bit)"
}}

function Test-GitInstalled {{
    $gitVersion = git --version 2>$null
    return -not [string]::IsNullOrWhiteSpace($gitVersion)
}}

function Resolve-OfflineGitInstaller {{
    if ([string]::IsNullOrWhiteSpace($offlineAssetsRoot)) {{
        return $null
    }}

    $gitDir = Join-Path $offlineAssetsRoot "git"
    if (-not (Test-Path $gitDir)) {{
        return $null
    }}

    $regex = Get-OfflineArchRegex
    $archMatch = Get-ChildItem -Path $gitDir -File -Filter "*.exe" -ErrorAction SilentlyContinue |
        Where-Object {{ $_.Name.ToLowerInvariant() -match $regex }} |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($archMatch) {{
        return $archMatch.FullName
    }}

    $fallback = Get-ChildItem -Path $gitDir -File -Filter "*.exe" -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($fallback) {{
        return $fallback.FullName
    }}

    return $null
}}

function Install-GitViaLocalInstaller {{
    param([string]$installerPath)

    try {{
        Write-Host "Installing Git from bundled installer: $installerPath"
        $proc = Start-Process -FilePath $installerPath -ArgumentList "/VERYSILENT /NORESTART /NOCANCEL /SP- /CLOSEAPPLICATIONS /RESTARTAPPLICATIONS" -Wait -PassThru
        if ($proc.ExitCode -eq 0 -or $proc.ExitCode -eq 3010) {{
            $env:PATH = "C:\Program Files\Git\cmd;C:\Program Files\Git\bin;$env:PATH"
            return $true
        }}
        Write-Warning "Bundled Git installer failed (exit code: $($proc.ExitCode))"
        return $false
    }} catch {{
        Write-Warning "Bundled Git installer failed: $($_.Exception.Message)"
        return $false
    }}
}}

function Install-GitViaWinget {{
    $hasWinget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $hasWinget) {{
        return $false
    }}

    Write-Host "Installing Git via winget fallback..."
    winget install --id Git.Git --source winget --exact --accept-source-agreements --accept-package-agreements
    if ($LASTEXITCODE -eq 0) {{
        return $true
    }}

    Write-Warning "winget Git install failed (exit code: $LASTEXITCODE), refreshing source and retrying..."
    winget source update winget
    if ($LASTEXITCODE -ne 0) {{
        winget source reset --force
    }}
    winget install --id Git.Git --source winget --exact --accept-source-agreements --accept-package-agreements
    return ($LASTEXITCODE -eq 0)
}}

function Ensure-GitAvailable {{
    if (Test-GitInstalled) {{
        return
    }}

    Write-Warning "Git is missing. Trying bundled offline Git installer first."
    $offlineGitInstaller = Resolve-OfflineGitInstaller
    if ($offlineGitInstaller) {{
        if (Install-GitViaLocalInstaller -installerPath $offlineGitInstaller -and (Test-GitInstalled)) {{
            Write-Host "Git installed successfully via bundled offline installer"
            return
        }}
    }}

    Write-Warning "Bundled Git install unavailable/failed. Trying winget fallback..."
    if (Install-GitViaWinget -and (Test-GitInstalled)) {{
        Write-Host "Git installed successfully via winget fallback"
        return
    }}

    Write-Warning "Git installation did not complete. OpenClaw installation may fail if Git is required."
}}

function Resolve-OfflineOpenClawPackage {{
    if ([string]::IsNullOrWhiteSpace($offlineAssetsRoot)) {{
        return $null
    }}

    $pkgDir = Join-Path $offlineAssetsRoot "openclaw"
    if (-not (Test-Path $pkgDir)) {{
        return $null
    }}

    $primary = Get-ChildItem -Path $pkgDir -File -Filter "openclaw-*.tgz" -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($primary) {{
        return $primary.FullName
    }}

    $fallback = Get-ChildItem -Path $pkgDir -File -Filter "*.tgz" -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($fallback) {{
        return $fallback.FullName
    }}

    return $null
}}

function Resolve-OfflineOpenClawCache {{
    if ([string]::IsNullOrWhiteSpace($offlineAssetsRoot)) {{
        return $null
    }}

    $cacheDir = Join-Path $offlineAssetsRoot "openclaw\npm-cache"
    if (-not (Test-Path $cacheDir)) {{
        return $null
    }}

    $cacheEntries = Get-ChildItem -Path $cacheDir -Force -ErrorAction SilentlyContinue
    if (-not $cacheEntries) {{
        return $null
    }}

    return $cacheDir
}}

$script:usedIgnoreScriptsFallback = $false
function Install-OpenClawPackageWithFallback {{
    param(
        [Parameter(Mandatory = $true)][string]$packageSpec,
        [string]$offlineCache = $null
    )

    $installArgs = @("install", "-g", "$packageSpec")
    $isOfflinePackage = $packageSpec -like "*.tgz"
    if ($isOfflinePackage -and -not [string]::IsNullOrWhiteSpace($offlineCache)) {{
        Write-Host "Installing OpenClaw package source with bundled npm cache: $packageSpec"
        npm @installArgs --cache "$offlineCache" --offline
        if ($LASTEXITCODE -eq 0) {{
            return $true
        }}

        Write-Warning "Offline npm install failed with exit code $LASTEXITCODE."
    }} else {{
        Write-Host "Installing OpenClaw package source: $packageSpec"
        npm @installArgs
        if ($LASTEXITCODE -eq 0) {{
            return $true
        }}
    }}

    $globalInstallExitCode = $LASTEXITCODE
    Write-Warning "Global npm install failed with exit code $globalInstallExitCode. Retrying with user-local npm prefix..."

    if (-not $env:LOCALAPPDATA) {{
        Write-Warning "LOCALAPPDATA is unavailable, cannot run Windows user-local fallback install"
        return $false
    }}

    $openclawPrefix = Join-Path $env:LOCALAPPDATA 'Programs\OpenClaw\npm-global'
    New-Item -ItemType Directory -Force -Path $openclawPrefix | Out-Null

    if ($isOfflinePackage -and -not [string]::IsNullOrWhiteSpace($offlineCache)) {{
        npm @installArgs --prefix "$openclawPrefix" --cache "$offlineCache" --offline
    }} else {{
        npm @installArgs --prefix "$openclawPrefix"
    }}
    if ($LASTEXITCODE -ne 0) {{
        $localInstallExitCode = $LASTEXITCODE
        Write-Warning "User-local npm install failed with exit code $localInstallExitCode. Retrying with --ignore-scripts..."
        if ($isOfflinePackage -and -not [string]::IsNullOrWhiteSpace($offlineCache)) {{
            npm @installArgs --prefix "$openclawPrefix" --cache "$offlineCache" --offline --ignore-scripts
        }} else {{
            npm @installArgs --prefix "$openclawPrefix" --ignore-scripts
        }}
        if ($LASTEXITCODE -ne 0) {{
            Write-Warning "Install source failed after retries (global=$globalInstallExitCode, local=$localInstallExitCode, ignoreScripts=$LASTEXITCODE)"
            return $false
        }}
        $script:usedIgnoreScriptsFallback = $true
    }}

    $env:PATH = "$openclawPrefix;$env:APPDATA\npm;$env:PATH"
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($userPath -notlike "*$openclawPrefix*") {{
        if ([string]::IsNullOrWhiteSpace($userPath)) {{
            $newPath = $openclawPrefix
        }} else {{
            $newPath = "$userPath;$openclawPrefix"
        }}
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        Write-Host "Added OpenClaw npm fallback directory to user PATH: $openclawPrefix"
    }}

    return $true
}}

Ensure-GitAvailable

$installSources = @()
$offlineInstallEnforced = $false
$offlineOpenClawPackage = Resolve-OfflineOpenClawPackage
$offlineOpenClawCache = Resolve-OfflineOpenClawCache
if ($offlineOpenClawPackage) {{
    Write-Host "Detected bundled offline OpenClaw package: $offlineOpenClawPackage"
    if (-not $offlineOpenClawCache) {{
        throw "Bundled offline OpenClaw npm cache is missing. Rebuild installer with openclaw/npm-cache assets."
    }}
    Write-Host "Detected bundled offline OpenClaw npm cache: $offlineOpenClawCache"
    $offlineInstallEnforced = $true
    $installSources += @{{ packageSpec = $offlineOpenClawPackage; offlineCache = $offlineOpenClawCache }}
}}
if (-not $offlineInstallEnforced) {{
    $installSources += @{{ packageSpec = "openclaw@latest"; offlineCache = $null }}
}}

Write-Host "Installing OpenClaw using npm..."
$openclawInstalled = $false
foreach ($source in $installSources) {{
    if (Install-OpenClawPackageWithFallback -packageSpec $source.packageSpec -offlineCache $source.offlineCache) {{
        $openclawInstalled = $true
        break
    }}
}}

if (-not $openclawInstalled) {{
    if ($offlineInstallEnforced) {{
        throw "OpenClaw offline install failed from bundled package/cache. Runtime online fallback is disabled."
    }}
    throw "OpenClaw install failed after all offline/online retries."
}}

if ($script:usedIgnoreScriptsFallback) {{
    Write-Warning "Installed OpenClaw with --ignore-scripts fallback. Optional native modules may need manual rebuild later."
}}

{find_openclaw}

$openclawVersion = & $openclawCmd --version 2>$null
if ($LASTEXITCODE -ne 0 -or -not $openclawVersion) {{
    throw "OpenClaw installation verification failed"
}}

Write-Host "OpenClaw installed successfully: $openclawVersion"
"#,
        git_setup = POWERSHELL_GIT_HTTPS_SETUP,
        offline_assets_root = offline_assets_root,
        find_openclaw = POWERSHELL_FIND_OPENCLAW,
    );

    match shell::run_powershell_output(&script) {
        Ok(output) => {
            if get_openclaw_version().is_some() {
                Ok(InstallResult {
                    success: true,
                    message: "OpenClaw installed successfully!".to_string(),
                    error: None,
                })
            } else {
                Ok(InstallResult {
                    success: false,
                    message: "Application restart required after installation".to_string(),
                    error: Some(output),
                })
            }
        }
        Err(e) => {
            let lower = e.to_lowercase();
            let message = if lower.contains("node-llama-cpp")
                || lower.contains("3221225477")
                || lower.contains("-1073741819")
            {
                "OpenClaw installation failed in native dependency setup. The app already retried a Windows fallback install. You can rerun later after checking antivirus/Windows build tools, or run: npm install -g openclaw@latest --prefix \"$env:LOCALAPPDATA\\Programs\\OpenClaw\\npm-global\" --ignore-scripts"
            } else {
                "OpenClaw installation failed"
            };

            Ok(InstallResult {
                success: false,
                message: message.to_string(),
                error: Some(e),
            })
        }
    }
}

/// Install OpenClaw on Unix systems
async fn install_openclaw_unix() -> Result<InstallResult, String> {
    let script = format!(
        r#"
set -e
{git_setup}

# Check Node.js
if ! command -v node &> /dev/null; then
    echo "Error: Please install Node.js first" >&2
    exit 1
fi

echo "Installing OpenClaw using npm..."
npm install -g openclaw@latest

{find_openclaw}
"$OPENCLAW_BIN" --version
"#,
        git_setup = BASH_GIT_HTTPS_SETUP,
        find_openclaw = BASH_FIND_OPENCLAW,
    );

    match shell::run_bash_output(&script) {
        Ok(output) => Ok(InstallResult {
            success: true,
            message: format!("OpenClaw installed successfully! {}", output),
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            success: false,
            message: "OpenClaw installation failed".to_string(),
            error: Some(e),
        }),
    }
}

/// Initialize OpenClaw configuration
#[command]
pub async fn init_openclaw_config() -> Result<InstallResult, String> {
    info!("[Init Config] Starting OpenClaw configuration initialization...");

    let config_dir = platform::get_config_dir();
    info!("[Init Config] Config directory: {}", config_dir);

    // Create config directory
    info!("[Init Config] Creating config directory...");
    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        error!("[Init Config] Failed to create config directory: {}", e);
        return Ok(InstallResult {
            success: false,
            message: "Failed to create config directory".to_string(),
            error: Some(e.to_string()),
        });
    }

    // Create subdirectories
    let subdirs = ["agents/main/sessions", "agents/main/agent", "credentials"];
    for subdir in subdirs {
        let path = format!("{}/{}", config_dir, subdir);
        info!("[Init Config] Creating subdirectory: {}", subdir);
        if let Err(e) = std::fs::create_dir_all(&path) {
            error!(
                "[Init Config] Failed to create directory: {} - {}",
                subdir, e
            );
            return Ok(InstallResult {
                success: false,
                message: format!("Failed to create directory: {}", subdir),
                error: Some(e.to_string()),
            });
        }
    }

    // Set config directory permissions to 700 (consistent with shell script chmod 700)
    // Only execute on Unix systems
    #[cfg(unix)]
    {
        info!("[Init Config] Setting directory permissions to 700...");
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&config_dir) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o700);
            if let Err(e) = std::fs::set_permissions(&config_dir, perms) {
                warn!("[Init Config] Failed to set permissions: {}", e);
            } else {
                info!("[Init Config] Permissions set successfully");
            }
        }
    }

    // Set gateway mode to local
    info!("[Init Config] Executing: openclaw config set gateway.mode local");
    let result = shell::run_openclaw(&["config", "set", "gateway.mode", "local"]);

    // Also set controlUi.allowInsecureAuth for local manager (skip device pairing)
    info!("[Init Config] Executing: openclaw config set gateway.controlUi.allowInsecureAuth true");
    let _ = shell::run_openclaw(&[
        "config",
        "set",
        "gateway.controlUi.allowInsecureAuth",
        "true",
    ]);

    match result {
        Ok(output) => {
            info!("[Init Config] Configuration initialized successfully");
            debug!(
                "[Init Config] Command output: {}",
                log_sanitizer::sanitize(&output)
            );
            Ok(InstallResult {
                success: true,
                message: "Configuration initialized successfully!".to_string(),
                error: None,
            })
        }
        Err(e) => {
            error!("[Init Config] Configuration initialization failed: {}", e);
            Ok(InstallResult {
                success: false,
                message: "Configuration initialization failed".to_string(),
                error: Some(e),
            })
        }
    }
}

/// Open terminal to execute installation script (for scenarios requiring administrator privileges)
#[command]
pub async fn open_install_terminal(install_type: String) -> Result<String, String> {
    match install_type.as_str() {
        "nodejs" => open_nodejs_install_terminal().await,
        "openclaw" => open_openclaw_install_terminal().await,
        _ => Err(format!("Unknown installation type: {}", install_type)),
    }
}

/// Open terminal to install Node.js
async fn open_nodejs_install_terminal() -> Result<String, String> {
    if platform::is_windows() {
        // Windows: Open PowerShell to execute installation
        let script = r#"
Start-Process powershell -ArgumentList '-NoExit', '-Command', '
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "    Node.js Installation Wizard" -ForegroundColor White
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

function Get-NodeArchToken {
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {
        return "arm64"
    }
    if ([Environment]::Is64BitOperatingSystem) {
        return "x64"
    }
    return "x86"
}

function Resolve-LatestNode22Version {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $index = Invoke-RestMethod -Uri "https://nodejs.org/dist/index.json" -UseBasicParsing
    $latest22 = $index | Where-Object { $_.version -like "v22.*" } | Select-Object -First 1
    if (-not $latest22) {
        throw "Unable to resolve latest Node.js v22 release from nodejs.org"
    }
    return $latest22.version.TrimStart("v")
}

$installSuccess = $false

$hasWinget = Get-Command winget -ErrorAction SilentlyContinue
if ($hasWinget) {
    Write-Host "Installing Node.js 22 using winget (source: winget)..." -ForegroundColor Yellow
    winget install --id OpenJS.NodeJS.LTS --source winget --exact --accept-source-agreements --accept-package-agreements
    if ($LASTEXITCODE -eq 0) {
        $installSuccess = $true
    } else {
        Write-Host "winget install failed (exit code: $LASTEXITCODE), refreshing sources..." -ForegroundColor Yellow
        winget source update winget
        if ($LASTEXITCODE -ne 0) {
            winget source reset --force
        }
        winget install --id OpenJS.NodeJS.LTS --source winget --exact --accept-source-agreements --accept-package-agreements
        if ($LASTEXITCODE -eq 0) {
            $installSuccess = $true
        }
    }
}

if (-not $installSuccess) {
    Write-Host "Trying official Node.js MSI fallback..." -ForegroundColor Yellow
    try {
        $version = Resolve-LatestNode22Version
        $arch = Get-NodeArchToken
        $msiUrl = "https://nodejs.org/dist/v$version/node-v$version-$arch.msi"
        $msiPath = Join-Path $env:TEMP "node-v$version-$arch.msi"

        Invoke-WebRequest -Uri $msiUrl -OutFile $msiPath -UseBasicParsing
        & msiexec.exe /i $msiPath /qn /norestart
        $msiExitCode = $LASTEXITCODE
        if ($msiExitCode -eq 0 -or $msiExitCode -eq 3010) {
            $installSuccess = $true
        } else {
            Write-Host "MSI install failed (exit code: $msiExitCode)" -ForegroundColor Red
        }
    } catch {
        Write-Host "MSI fallback failed: $($_.Exception.Message)" -ForegroundColor Red
    }
}

if (-not $installSuccess) {
    Write-Host "Please download and install Node.js from:" -ForegroundColor Yellow
    Write-Host "https://nodejs.org/en/download" -ForegroundColor Green
    Write-Host ""
    Start-Process "https://nodejs.org/en/download"
} else {
    $nodeVersion = node --version 2>$null
    if ($nodeVersion) {
        Write-Host "Installed Node.js: $nodeVersion" -ForegroundColor Green
    }
}

Write-Host ""
Write-Host "Please restart OpenClaw小白安装工具 after installation" -ForegroundColor Green
Write-Host ""
Read-Host "Press Enter to close this window"
' -Verb RunAs
"#;
        shell::run_powershell_output(script)?;
        Ok("Installation terminal opened".to_string())
    } else if platform::is_macos() {
        // macOS: Open Terminal.app
        let script_content = r#"#!/bin/bash
clear
echo "========================================"
echo "    Node.js Installation Wizard"
echo "========================================"
echo ""

# Check Homebrew
if ! command -v brew &> /dev/null; then
    echo "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

    if [[ -f /opt/homebrew/bin/brew ]]; then
        eval "$(/opt/homebrew/bin/brew shellenv)"
    elif [[ -f /usr/local/bin/brew ]]; then
        eval "$(/usr/local/bin/brew shellenv)"
    fi
fi

echo "Installing Node.js 22..."
brew install node@22
brew link --overwrite node@22

echo ""
echo "Installation complete!"
node --version
echo ""
read -p "Press Enter to close this window..."
"#;

        let script_path = "/tmp/openclaw_install_nodejs.command";
        std::fs::write(script_path, script_content)
            .map_err(|e| format!("Failed to create script: {}", e))?;

        std::process::Command::new("chmod")
            .args(["+x", script_path])
            .output()
            .map_err(|e| format!("Failed to set permissions: {}", e))?;

        std::process::Command::new("open")
            .arg(script_path)
            .spawn()
            .map_err(|e| format!("Failed to launch terminal: {}", e))?;

        Ok("Installation terminal opened".to_string())
    } else {
        Err("Please install Node.js manually: https://nodejs.org/".to_string())
    }
}

/// Open terminal to install OpenClaw
async fn open_openclaw_install_terminal() -> Result<String, String> {
    if platform::is_windows() {
        let terminal_script = format!(
            r#"
$ErrorActionPreference = 'Stop'
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "    OpenClaw Installation Wizard" -ForegroundColor White
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

{git_setup}

try {{
    Write-Host "Installing OpenClaw..." -ForegroundColor Yellow
    npm install -g openclaw@latest
    if ($LASTEXITCODE -ne 0) {{
        throw "npm install -g openclaw@latest failed with exit code $LASTEXITCODE"
    }}

    {find_openclaw}

    Write-Host ""
    Write-Host "Initializing configuration..." -ForegroundColor Yellow
    & $openclawCmd config set gateway.mode local
    if ($LASTEXITCODE -ne 0) {{
        throw "Failed to set gateway.mode local"
    }}

    & $openclawCmd config set gateway.controlUi.allowInsecureAuth true
    if ($LASTEXITCODE -ne 0) {{
        throw "Failed to set gateway.controlUi.allowInsecureAuth"
    }}

    $configRoot = Join-Path $HOME '.openclaw'
    New-Item -ItemType Directory -Force -Path (Join-Path $configRoot 'agents\main\sessions') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $configRoot 'agents\main\agent') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $configRoot 'credentials') | Out-Null

    $openclawVersion = & $openclawCmd --version
    if ($LASTEXITCODE -ne 0 -or -not $openclawVersion) {{
        throw "OpenClaw installation verification failed"
    }}

    Write-Host ""
    Write-Host "Installation complete!" -ForegroundColor Green
    Write-Host $openclawVersion -ForegroundColor Green
}} catch {{
    Write-Host ""
    Write-Host "Installation failed: $($_.Exception.Message)" -ForegroundColor Red
}}

Write-Host ""
Read-Host "Press Enter to close this window"
"#,
            git_setup = POWERSHELL_GIT_HTTPS_SETUP,
            find_openclaw = POWERSHELL_FIND_OPENCLAW,
        );
        let launcher_script = format!(
            r#"
$scriptPath = Join-Path $env:TEMP 'openclaw_install_openclaw.ps1'
@'
{terminal_script}
'@ | Set-Content -Path $scriptPath -Encoding UTF8
Start-Process powershell -ArgumentList '-NoExit', '-ExecutionPolicy', 'Bypass', '-File', $scriptPath
"#,
            terminal_script = terminal_script,
        );
        shell::run_powershell_output(&launcher_script)?;
        Ok("Installation terminal opened".to_string())
    } else if platform::is_macos() {
        let script_content = format!(
            r#"#!/bin/bash
set -e
trap 'status=$?; if [ "$status" -ne 0 ]; then echo ""; echo "Installation failed. See messages above."; echo ""; read -p "Press Enter to close this window..."; fi' EXIT
clear
echo "========================================"
echo "    OpenClaw Installation Wizard"
echo "========================================"
echo ""

{git_setup}

echo "Installing OpenClaw..."
npm install -g openclaw@latest

{find_openclaw}

echo ""
echo "Initializing configuration..."
"$OPENCLAW_BIN" config set gateway.mode local
"$OPENCLAW_BIN" config set gateway.controlUi.allowInsecureAuth true

mkdir -p ~/.openclaw/agents/main/sessions
mkdir -p ~/.openclaw/agents/main/agent
mkdir -p ~/.openclaw/credentials

echo ""
echo "Installation complete!"
"$OPENCLAW_BIN" --version
echo ""
read -p "Press Enter to close this window..."
"#,
            git_setup = BASH_GIT_HTTPS_SETUP,
            find_openclaw = BASH_FIND_OPENCLAW,
        );

        let script_path = "/tmp/openclaw_install_openclaw.command";
        std::fs::write(script_path, script_content)
            .map_err(|e| format!("Failed to create script: {}", e))?;

        std::process::Command::new("chmod")
            .args(["+x", script_path])
            .output()
            .map_err(|e| format!("Failed to set permissions: {}", e))?;

        std::process::Command::new("open")
            .arg(script_path)
            .spawn()
            .map_err(|e| format!("Failed to launch terminal: {}", e))?;

        Ok("Installation terminal opened".to_string())
    } else {
        // Linux
        let script_content = format!(
            r#"#!/bin/bash
set -e
trap 'status=$?; if [ "$status" -ne 0 ]; then echo ""; echo "Installation failed. See messages above."; echo ""; read -p "Press Enter to close..."; fi' EXIT
clear
echo "========================================"
echo "    OpenClaw Installation Wizard"
echo "========================================"
echo ""

{git_setup}

echo "Installing OpenClaw..."
npm install -g openclaw@latest

{find_openclaw}

echo ""
echo "Initializing configuration..."
"$OPENCLAW_BIN" config set gateway.mode local
"$OPENCLAW_BIN" config set gateway.controlUi.allowInsecureAuth true

mkdir -p ~/.openclaw/agents/main/sessions
mkdir -p ~/.openclaw/agents/main/agent
mkdir -p ~/.openclaw/credentials

echo ""
echo "Installation complete!"
"$OPENCLAW_BIN" --version
echo ""
read -p "Press Enter to close..."
"#,
            git_setup = BASH_GIT_HTTPS_SETUP,
            find_openclaw = BASH_FIND_OPENCLAW,
        );

        let script_path = "/tmp/openclaw_install_openclaw.sh";
        std::fs::write(script_path, script_content)
            .map_err(|e| format!("Failed to create script: {}", e))?;

        std::process::Command::new("chmod")
            .args(["+x", script_path])
            .output()
            .map_err(|e| format!("Failed to set permissions: {}", e))?;

        // Try different terminals
        let terminals = ["gnome-terminal", "xfce4-terminal", "konsole", "xterm"];
        for term in terminals {
            if std::process::Command::new(term)
                .args(["--", script_path])
                .spawn()
                .is_ok()
            {
                return Ok("Installation terminal opened".to_string());
            }
        }

        Err("Unable to launch terminal, please run manually: npm install -g openclaw".to_string())
    }
}

/// Uninstall OpenClaw
#[command]
pub async fn uninstall_openclaw() -> Result<InstallResult, String> {
    info!("[Uninstall OpenClaw] Starting OpenClaw uninstallation...");
    let os = platform::get_os();
    info!("[Uninstall OpenClaw] Detected operating system: {}", os);

    // Stop service first
    info!("[Uninstall OpenClaw] Attempting to stop service...");
    let _ = shell::run_openclaw(&["gateway", "stop"]);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let result = match os.as_str() {
        "windows" => {
            info!("[Uninstall OpenClaw] Using Windows uninstallation method...");
            uninstall_openclaw_windows().await
        }
        _ => {
            info!("[Uninstall OpenClaw] Using Unix uninstallation method (npm)...");
            uninstall_openclaw_unix().await
        }
    };

    // After npm uninstall, delete the .openclaw config directory
    if let Some(home) = dirs::home_dir() {
        let openclaw_dir = home.join(".openclaw");
        if openclaw_dir.exists() {
            info!(
                "[Uninstall OpenClaw] Deleting .openclaw directory: {:?}",
                openclaw_dir
            );
            match std::fs::remove_dir_all(&openclaw_dir) {
                Ok(_) => info!("[Uninstall OpenClaw] Successfully deleted .openclaw directory"),
                Err(e) => warn!(
                    "[Uninstall OpenClaw] Failed to delete .openclaw directory: {}",
                    e
                ),
            }
        } else {
            info!("[Uninstall OpenClaw] .openclaw directory does not exist, skipping");
        }
    } else {
        warn!(
            "[Uninstall OpenClaw] Could not determine home directory, skipping .openclaw deletion"
        );
    }

    match &result {
        Ok(r) if r.success => info!("[Uninstall OpenClaw] Uninstallation successful"),
        Ok(r) => warn!("[Uninstall OpenClaw] Uninstallation failed: {}", r.message),
        Err(e) => error!("[Uninstall OpenClaw] Uninstallation error: {}", e),
    }

    result
}

/// Uninstall OpenClaw on Windows
async fn uninstall_openclaw_windows() -> Result<InstallResult, String> {
    // Use cmd.exe to execute npm uninstall to avoid PowerShell execution policy issues
    info!("[Uninstall OpenClaw] Executing npm uninstall -g openclaw...");

    match shell::run_cmd_output("npm uninstall -g openclaw") {
        Ok(output) => {
            info!("[Uninstall OpenClaw] npm output: {}", output);

            // Verify uninstallation was successful
            std::thread::sleep(std::time::Duration::from_millis(500));
            if get_openclaw_version().is_none() {
                Ok(InstallResult {
                    success: true,
                    message: "OpenClaw has been successfully uninstalled!".to_string(),
                    error: None,
                })
            } else {
                Ok(InstallResult {
                    success: false,
                    message: "Uninstall command executed but OpenClaw still exists, please try manual uninstallation".to_string(),
                    error: Some(output),
                })
            }
        }
        Err(e) => {
            warn!("[Uninstall OpenClaw] npm uninstall failed: {}", e);
            Ok(InstallResult {
                success: false,
                message: "OpenClaw uninstallation failed".to_string(),
                error: Some(e),
            })
        }
    }
}

/// Uninstall OpenClaw on Unix systems
async fn uninstall_openclaw_unix() -> Result<InstallResult, String> {
    let script = r#"
echo "Uninstalling OpenClaw..."
npm uninstall -g openclaw

# Verify uninstallation
if command -v openclaw &> /dev/null; then
    echo "Warning: openclaw command still exists"
    exit 1
else
    echo "OpenClaw has been successfully uninstalled"
    exit 0
fi
"#;

    match shell::run_bash_output(script) {
        Ok(output) => Ok(InstallResult {
            success: true,
            message: format!("OpenClaw has been successfully uninstalled! {}", output),
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            success: false,
            message: "OpenClaw uninstallation failed".to_string(),
            error: Some(e),
        }),
    }
}

/// Version update information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    /// Whether an update is available
    pub update_available: bool,
    /// Current version
    pub current_version: Option<String>,
    /// Latest version
    pub latest_version: Option<String>,
    /// Error message
    pub error: Option<String>,
}

/// Check for OpenClaw updates
#[command]
pub async fn check_openclaw_update() -> Result<UpdateInfo, String> {
    info!("[Version Check] Starting OpenClaw update check...");

    // Get current version
    let current_version = get_openclaw_version();
    info!("[Version Check] Current version: {:?}", current_version);

    if current_version.is_none() {
        info!("[Version Check] OpenClaw is not installed");
        return Ok(UpdateInfo {
            update_available: false,
            current_version: None,
            latest_version: None,
            error: Some("OpenClaw is not installed".to_string()),
        });
    }

    // Get latest version
    let latest_version = get_latest_openclaw_version();
    info!("[Version Check] Latest version: {:?}", latest_version);

    if latest_version.is_none() {
        return Ok(UpdateInfo {
            update_available: false,
            current_version,
            latest_version: None,
            error: Some("Unable to get latest version information".to_string()),
        });
    }

    // Compare versions
    let current = current_version.clone().unwrap();
    let latest = latest_version.clone().unwrap();
    let update_available = compare_versions(&current, &latest);

    info!("[Version Check] Update available: {}", update_available);

    Ok(UpdateInfo {
        update_available,
        current_version,
        latest_version,
        error: None,
    })
}

/// Get the latest version from npm registry
fn get_latest_openclaw_version() -> Option<String> {
    // Use npm view to get the latest version
    let result = if platform::is_windows() {
        shell::run_cmd_output("npm view openclaw version")
    } else {
        shell::run_bash_output("npm view openclaw version 2>/dev/null")
    };

    match result {
        Ok(version) => {
            let v = version.trim().to_string();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        Err(e) => {
            warn!("[Version Check] Failed to get latest version: {}", e);
            None
        }
    }
}

/// Compare version numbers, return whether an update is available
/// current: Current version (e.g. "1.0.0" or "v1.0.0")
/// latest: Latest version (e.g. "1.0.1")
fn compare_versions(current: &str, latest: &str) -> bool {
    // Remove possible 'v' prefix and whitespace
    let current = current.trim().trim_start_matches('v');
    let latest = latest.trim().trim_start_matches('v');

    // Helper to parse version string into numeric parts (splitting by . and -)
    let parse_version = |v: &str| -> Vec<u32> {
        v.split(|c| c == '.' || c == '-')
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let current_parts = parse_version(current);
    let latest_parts = parse_version(latest);

    // Compare each part
    let max_len = std::cmp::max(current_parts.len(), latest_parts.len());
    for i in 0..max_len {
        let c = current_parts.get(i).unwrap_or(&0);
        let l = latest_parts.get(i).unwrap_or(&0);
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }

    false
}

/// Update OpenClaw
#[command]
pub async fn update_openclaw() -> Result<InstallResult, String> {
    info!("[Update OpenClaw] Starting OpenClaw update...");
    let os = platform::get_os();

    // Stop service first
    info!("[Update OpenClaw] Attempting to stop service...");
    let _ = shell::run_openclaw(&["gateway", "stop"]);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let result = match os.as_str() {
        "windows" => {
            info!("[Update OpenClaw] Using Windows update method...");
            update_openclaw_windows().await
        }
        _ => {
            info!("[Update OpenClaw] Using Unix update method (npm)...");
            update_openclaw_unix().await
        }
    };

    match &result {
        Ok(r) if r.success => info!("[Update OpenClaw] Update successful"),
        Ok(r) => warn!("[Update OpenClaw] Update failed: {}", r.message),
        Err(e) => error!("[Update OpenClaw] Update error: {}", e),
    }

    result
}

/// Update OpenClaw on Windows
async fn update_openclaw_windows() -> Result<InstallResult, String> {
    info!("[Update OpenClaw] Executing npm install -g openclaw@latest...");

    let offline_assets_root = resolve_windows_offline_assets_root();
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
{git_setup}

$nodeVersion = node --version 2>$null
if (-not $nodeVersion) {{
    throw "Please install Node.js first"
}}

function Resolve-OfflineOpenClawPackage {{
    if ([string]::IsNullOrWhiteSpace($offlineAssetsRoot)) {{
        return $null
    }}

    $pkgDir = Join-Path $offlineAssetsRoot "openclaw"
    if (-not (Test-Path $pkgDir)) {{
        return $null
    }}

    $primary = Get-ChildItem -Path $pkgDir -File -Filter "openclaw-*.tgz" -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($primary) {{
        return $primary.FullName
    }}

    $fallback = Get-ChildItem -Path $pkgDir -File -Filter "*.tgz" -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($fallback) {{
        return $fallback.FullName
    }}

    return $null
}}

function Resolve-OfflineOpenClawCache {{
    if ([string]::IsNullOrWhiteSpace($offlineAssetsRoot)) {{
        return $null
    }}

    $cacheDir = Join-Path $offlineAssetsRoot "openclaw\npm-cache"
    if (-not (Test-Path $cacheDir)) {{
        return $null
    }}

    $cacheEntries = Get-ChildItem -Path $cacheDir -Force -ErrorAction SilentlyContinue
    if (-not $cacheEntries) {{
        return $null
    }}

    return $cacheDir
}}

function Install-OpenClawUpdateWithFallback {{
    $offlinePackage = Resolve-OfflineOpenClawPackage
    $offlineCache = Resolve-OfflineOpenClawCache

    if ($offlinePackage) {{
        Write-Host "Updating OpenClaw from bundled package: $offlinePackage"
        if ($offlineCache) {{
            npm install -g "$offlinePackage" --cache "$offlineCache" --offline
            if ($LASTEXITCODE -eq 0) {{
                return
            }}

            Write-Warning "Offline update failed with exit code $LASTEXITCODE. Retrying with --prefer-offline..."
            npm install -g "$offlinePackage" --cache "$offlineCache" --prefer-offline
            if ($LASTEXITCODE -eq 0) {{
                return
            }}
        }}

        Write-Warning "Bundled package update failed. Falling back to online update..."
    }}

    npm install -g openclaw@latest
    if ($LASTEXITCODE -ne 0) {{
        throw "npm install -g openclaw update failed with exit code $LASTEXITCODE"
    }}
}}

$offlineAssetsRoot = @'
{offline_assets_root}
'@

Write-Host "Updating OpenClaw..."
Install-OpenClawUpdateWithFallback

{find_openclaw}
$openclawVersion = & $openclawCmd --version 2>$null
if ($LASTEXITCODE -ne 0 -or -not $openclawVersion) {{
    throw "OpenClaw update verification failed"
}}

Write-Host $openclawVersion
"#,
        git_setup = POWERSHELL_GIT_HTTPS_SETUP,
        find_openclaw = POWERSHELL_FIND_OPENCLAW,
        offline_assets_root = offline_assets_root,
    );

    match shell::run_powershell_output(&script) {
        Ok(output) => {
            info!("[Update OpenClaw] npm output: {}", output);

            // Get new version
            let new_version = get_openclaw_version();

            Ok(InstallResult {
                success: true,
                message: format!(
                    "OpenClaw has been updated to {}",
                    new_version.unwrap_or("latest version".to_string())
                ),
                error: None,
            })
        }
        Err(e) => {
            warn!("[Update OpenClaw] npm install failed: {}", e);
            Ok(InstallResult {
                success: false,
                message: "OpenClaw update failed".to_string(),
                error: Some(e),
            })
        }
    }
}

/// Update OpenClaw on Unix systems
async fn update_openclaw_unix() -> Result<InstallResult, String> {
    let script = format!(
        r#"
set -e
{git_setup}

echo "Updating OpenClaw..."
npm install -g openclaw@latest

{find_openclaw}
"$OPENCLAW_BIN" --version
"#,
        git_setup = BASH_GIT_HTTPS_SETUP,
        find_openclaw = BASH_FIND_OPENCLAW,
    );

    match shell::run_bash_output(&script) {
        Ok(output) => Ok(InstallResult {
            success: true,
            message: format!("OpenClaw has been updated! {}", output),
            error: None,
        }),
        Err(e) => Ok(InstallResult {
            success: false,
            message: "OpenClaw update failed".to_string(),
            error: Some(e),
        }),
    }
}
