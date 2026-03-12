#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import fs from "node:fs/promises";
import path from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..");

const supportedArchitectures = new Set(["x64", "arm64", "x86"]);

const defaultOptions = {
  outputDir: path.join(repoRoot, "offline-assets", "windows"),
  stageDir: path.join(repoRoot, "resources", "offline", "windows"),
  stage: true,
  force: false,
  nodeVersion: "latest-22",
  gitVersion: "latest",
  openclawVersion: "latest",
  architectures: ["x64"],
};

function printHelp() {
  console.log(`Prepare Windows offline bundle assets for Node.js + Git + OpenClaw.

Usage:
  node scripts/prepare-windows-offline-assets.mjs [options]

Options:
  --output-dir <path>         Output root (default: offline-assets/windows)
  --stage-dir <path>          Staging root (default: resources/offline/windows)
  --no-stage                  Do not copy prepared assets into stage dir
  --force                     Re-download/re-pack even if files already exist
  --node-version <spec>       Node.js version or latest-22 (default: latest-22)
  --git-version <spec>        Git for Windows tag or latest (default: latest)
  --openclaw-version <spec>   OpenClaw npm version or latest (default: latest)
  --architectures <csv>       Target arches: x64,arm64,x86 (default: x64)
  --help                      Show this help

Examples:
  node scripts/prepare-windows-offline-assets.mjs
  node scripts/prepare-windows-offline-assets.mjs --force --architectures x64,arm64
  node scripts/prepare-windows-offline-assets.mjs --no-stage --output-dir offline-assets/windows
`);
}

function parseArgs(argv) {
  const options = { ...defaultOptions };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];

    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }

    if (arg === "--no-stage") {
      options.stage = false;
      continue;
    }

    if (arg === "--force") {
      options.force = true;
      continue;
    }

    const next = argv[i + 1];
    if (!next || next.startsWith("--")) {
      throw new Error(`Missing value for ${arg}`);
    }

    if (arg === "--output-dir") {
      options.outputDir = path.resolve(next);
      i += 1;
      continue;
    }

    if (arg === "--stage-dir") {
      options.stageDir = path.resolve(next);
      i += 1;
      continue;
    }

    if (arg === "--node-version") {
      options.nodeVersion = next.trim();
      i += 1;
      continue;
    }

    if (arg === "--git-version") {
      options.gitVersion = next.trim();
      i += 1;
      continue;
    }

    if (arg === "--openclaw-version") {
      options.openclawVersion = next.trim();
      i += 1;
      continue;
    }

    if (arg === "--architectures") {
      const parsed = next
        .split(",")
        .map((item) => item.trim().toLowerCase())
        .filter(Boolean);
      options.architectures = parsed;
      i += 1;
      continue;
    }

    throw new Error(`Unknown option: ${arg}`);
  }

  if (!options.architectures.length) {
    throw new Error("--architectures cannot be empty");
  }

  const invalid = options.architectures.filter((arch) => !supportedArchitectures.has(arch));
  if (invalid.length) {
    throw new Error(`Unsupported architecture(s): ${invalid.join(", ")}`);
  }

  return options;
}

function log(message) {
  console.log(`[offline-assets] ${message}`);
}

function warn(message) {
  console.warn(`[offline-assets] WARN: ${message}`);
}

function githubHeaders() {
  const headers = {
    Accept: "application/vnd.github+json",
    "User-Agent": "openclaw-manager-offline-bundle-prep",
  };

  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  return headers;
}

async function pathExists(targetPath) {
  try {
    await fs.access(targetPath);
    return true;
  } catch {
    return false;
  }
}

async function ensureDir(targetPath) {
  await fs.mkdir(targetPath, { recursive: true });
}

async function readJson(url, headers = {}) {
  const response = await fetch(url, { headers, redirect: "follow" });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status} while requesting ${url}`);
  }
  return response.json();
}

async function downloadFile(url, destinationPath, force = false, headers = {}) {
  if (!force && (await pathExists(destinationPath))) {
    log(`Reuse existing file: ${path.relative(repoRoot, destinationPath)}`);
    return;
  }

  await ensureDir(path.dirname(destinationPath));

  const tempPath = `${destinationPath}.partial`;
  const response = await fetch(url, { headers, redirect: "follow" });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status} while downloading ${url}`);
  }
  if (!response.body) {
    throw new Error(`Empty response body while downloading ${url}`);
  }

  log(`Downloading ${url}`);

  try {
    await pipeline(Readable.fromWeb(response.body), createWriteStream(tempPath));
    await fs.rename(tempPath, destinationPath);
  } catch (error) {
    await fs.rm(tempPath, { force: true });
    throw error;
  }
}

function resolveCommand(command, options = {}) {
  if (process.platform === "win32" && command === "npm") {
    return {
      command,
      shell: options.shell ?? true,
    };
  }

  return {
    command,
    shell: options.shell ?? false,
  };
}

function runCommand(command, args, options = {}) {
  const resolved = resolveCommand(command, options);
  const result = spawnSync(resolved.command, args, {
    cwd: options.cwd || repoRoot,
    env: options.env || process.env,
    encoding: "utf8",
    stdio: options.stdio || ["ignore", "pipe", "pipe"],
    shell: resolved.shell,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    const stderr = (result.stderr || "").trim();
    const stdout = (result.stdout || "").trim();
    const details = stderr || stdout || `exit code ${result.status}`;
    throw new Error(`${command} ${args.join(" ")} failed: ${details}`);
  }

  return (result.stdout || "").trim();
}

async function resolveLatestNode22Version() {
  const url = "https://nodejs.org/dist/index.json";
  const list = await readJson(url);
  const latest22 = list.find((item) => typeof item.version === "string" && item.version.startsWith("v22."));
  if (!latest22) {
    throw new Error("Unable to find latest Node.js v22 release from nodejs.org");
  }
  return latest22.version.replace(/^v/, "");
}

async function resolveNodeVersion(spec) {
  const normalized = (spec || "").trim().toLowerCase();
  if (!normalized || normalized === "latest" || normalized === "latest-22" || normalized === "lts-22" || normalized === "22") {
    return resolveLatestNode22Version();
  }

  const value = spec.trim().replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+$/.test(value)) {
    throw new Error(`Invalid Node.js version spec: ${spec}`);
  }

  return value;
}

async function prepareNodeAssets(options, nodeDir) {
  const resolvedVersion = await resolveNodeVersion(options.nodeVersion);
  const baseUrl = `https://nodejs.org/dist/v${resolvedVersion}`;
  const selectedFiles = [];

  log(`Preparing Node.js assets for version ${resolvedVersion} (${options.architectures.join(", ")})`);

  for (const arch of options.architectures) {
    const msiName = `node-v${resolvedVersion}-${arch}.msi`;
    const zipName = `node-v${resolvedVersion}-win-${arch}.zip`;
    const msiPath = path.join(nodeDir, msiName);
    const zipPath = path.join(nodeDir, zipName);

    try {
      await downloadFile(`${baseUrl}/${msiName}`, msiPath, options.force);
      await downloadFile(`${baseUrl}/${zipName}`, zipPath, options.force);
      selectedFiles.push(msiPath, zipPath);
    } catch (error) {
      const message = String(error?.message || error);
      if (message.includes("HTTP 404")) {
        warn(`Node.js assets not found for architecture ${arch} (${resolvedVersion}), skipping this arch`);
        continue;
      }
      throw error;
    }
  }

  if (!selectedFiles.length) {
    throw new Error(`No Node.js offline assets were prepared for ${resolvedVersion}`);
  }

  return {
    version: resolvedVersion,
    files: selectedFiles,
  };
}

async function fetchGitRelease(versionSpec) {
  const normalized = (versionSpec || "").trim();
  const endpoint =
    !normalized || normalized.toLowerCase() === "latest"
      ? "https://api.github.com/repos/git-for-windows/git/releases/latest"
      : `https://api.github.com/repos/git-for-windows/git/releases/tags/${encodeURIComponent(normalized.startsWith("v") ? normalized : `v${normalized}`)}`;

  return readJson(endpoint, githubHeaders());
}

function selectGitAsset(release, arch) {
  const patterns = {
    x64: /^Git-.*-64-bit\.exe$/i,
    arm64: /^Git-.*-arm64\.exe$/i,
    x86: /^Git-.*-32-bit\.exe$/i,
  };

  const match = release.assets
    ?.slice()
    .sort((a, b) => String(a.name).localeCompare(String(b.name)))
    .find((asset) => patterns[arch].test(String(asset.name || "")));

  return match || null;
}

async function prepareGitAssets(options, gitDir) {
  const release = await fetchGitRelease(options.gitVersion);
  const releaseTag = String(release.tag_name || "").replace(/^v/, "");
  const selectedFiles = [];
  if (!releaseTag) {
    throw new Error("Unable to resolve Git for Windows release tag");
  }

  log(`Preparing Git for Windows assets for release ${releaseTag} (${options.architectures.join(", ")})`);

  for (const arch of options.architectures) {
    const asset = selectGitAsset(release, arch);
    if (!asset) {
      warn(`No Git for Windows installer found for architecture ${arch} in release ${releaseTag}`);
      continue;
    }

    const destinationPath = path.join(gitDir, asset.name);
    await downloadFile(asset.browser_download_url, destinationPath, options.force, githubHeaders());
    selectedFiles.push(destinationPath);
  }

  if (!selectedFiles.length) {
    throw new Error(`No Git for Windows installers were prepared for ${releaseTag}`);
  }

  return {
    version: releaseTag,
    files: selectedFiles,
  };
}

function resolveOpenClawVersion(spec) {
  const normalized = (spec || "").trim();
  const pkg = !normalized || normalized.toLowerCase() === "latest" ? "openclaw" : `openclaw@${normalized}`;

  const raw = runCommand("npm", ["view", pkg, "version"]);
  const version = raw.replace(/^"|"$/g, "").trim();
  if (!version) {
    throw new Error(`Unable to resolve OpenClaw version from npm for spec: ${spec}`);
  }

  return version;
}

async function prepareOpenClawAsset(options, openclawDir) {
  const resolvedVersion = resolveOpenClawVersion(options.openclawVersion);
  const tarballName = `openclaw-${resolvedVersion}.tgz`;
  const tarballPath = path.join(openclawDir, tarballName);

  if (!options.force && (await pathExists(tarballPath))) {
    log(`Reuse existing file: ${path.relative(repoRoot, tarballPath)}`);
    return {
      version: resolvedVersion,
      files: [tarballPath],
    };
  }

  log(`Packing OpenClaw npm tarball for version ${resolvedVersion}`);
  runCommand("npm", ["pack", `openclaw@${resolvedVersion}`, "--pack-destination", openclawDir, "--silent"]);

  if (!(await pathExists(tarballPath))) {
    throw new Error(`Expected tarball not found after npm pack: ${tarballPath}`);
  }

  return {
    version: resolvedVersion,
    files: [tarballPath],
  };
}

async function removeMatchingFiles(directory, matcher) {
  if (!(await pathExists(directory))) {
    return;
  }

  const entries = await fs.readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(directory, entry.name);

    if (entry.isDirectory()) {
      await fs.rm(fullPath, { recursive: true, force: true });
      continue;
    }

    if (entry.name === ".gitkeep" || entry.name === "README.md") {
      continue;
    }

    if (matcher(entry.name)) {
      await fs.rm(fullPath, { force: true });
    }
  }
}

async function copyFilesFlat(filePaths, targetDir) {
  await ensureDir(targetDir);
  for (const sourcePath of filePaths) {
    if (!(await pathExists(sourcePath))) {
      continue;
    }
    const targetPath = path.join(targetDir, path.basename(sourcePath));
    await fs.copyFile(sourcePath, targetPath);
  }
}

async function copyFirstFileAs(filePaths, targetPath) {
  const sourcePath = filePaths.find(Boolean);
  if (!sourcePath || !(await pathExists(sourcePath))) {
    return;
  }

  await ensureDir(path.dirname(targetPath));
  await fs.copyFile(sourcePath, targetPath);
}

async function stagePreparedAssets(stageDir, assets) {
  const nodeTarget = path.join(stageDir, "node");
  const gitTarget = path.join(stageDir, "git");
  const openclawTarget = path.join(stageDir, "openclaw");

  await ensureDir(stageDir);
  await ensureDir(nodeTarget);
  await ensureDir(gitTarget);
  await ensureDir(openclawTarget);

  await removeMatchingFiles(nodeTarget, (name) => /\.(msi|zip)$/i.test(name));
  await removeMatchingFiles(gitTarget, (name) => /\.exe$/i.test(name));
  await removeMatchingFiles(openclawTarget, (name) => /\.tgz$/i.test(name));

  await copyFilesFlat(assets.node, nodeTarget);
  await copyFilesFlat(assets.git, gitTarget);
  await copyFilesFlat(assets.openclaw, openclawTarget);
  await copyFirstFileAs(assets.openclaw, path.join(openclawTarget, "openclaw.tgz"));
}

async function summarizeGeneratedAssets(files) {
  const filtered = files.filter((filePath) => /\.(msi|zip|exe|tgz)$/i.test(filePath));
  if (!filtered.length) {
    warn("No asset files were prepared (.msi/.zip/.exe/.tgz)");
    return;
  }

  log("Prepared assets:");
  for (const filePath of filtered.sort()) {
    const stat = await fs.stat(filePath);
    const sizeMb = (stat.size / (1024 * 1024)).toFixed(2);
    console.log(`  - ${path.relative(repoRoot, filePath)} (${sizeMb} MB)`);
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));

  const nodeDir = path.join(options.outputDir, "node");
  const gitDir = path.join(options.outputDir, "git");
  const openclawDir = path.join(options.outputDir, "openclaw");

  await ensureDir(nodeDir);
  await ensureDir(gitDir);
  await ensureDir(openclawDir);

  const nodeAssets = await prepareNodeAssets(options, nodeDir);
  const gitAssets = await prepareGitAssets(options, gitDir);
  const openclawAssets = await prepareOpenClawAsset(options, openclawDir);

  const allPreparedFiles = [...nodeAssets.files, ...gitAssets.files, ...openclawAssets.files];

  if (options.stage) {
    await stagePreparedAssets(options.stageDir, {
      node: nodeAssets.files,
      git: gitAssets.files,
      openclaw: openclawAssets.files,
    });
    log(`Staged assets into: ${path.relative(repoRoot, options.stageDir)}`);
  } else {
    log("Skipping staging (--no-stage)");
  }

  log(`Resolved versions: node=${nodeAssets.version}, git=${gitAssets.version}, openclaw=${openclawAssets.version}`);
  await summarizeGeneratedAssets(allPreparedFiles);
}

main().catch((error) => {
  console.error(`[offline-assets] ERROR: ${error?.message || error}`);
  process.exit(1);
});
