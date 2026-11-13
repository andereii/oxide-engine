#!/usr/bin/env node
/**
 * Downloads the latest `oxide-windows` artifact from GitHub Actions
 * and places `oxide.exe` (and `oxide.pdb` if present) into `./bin/`.
 *
 * Usage:
 *   node scripts/fetch-bin.mjs [--branch <name>] [--run-id <id>]
 *
 * Authentication (in order of precedence):
 *   1. `gh` CLI if installed and authenticated
 *   2. `GITHUB_TOKEN` or `GH_TOKEN` environment variable (or in `.env`)
 */

import { execFileSync, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const BIN_DIR = path.join(ROOT, "bin");
const WORKFLOW_FILE = "build.yml";
const ARTIFACT_NAME = "oxide-windows";

function parseArgs(argv) {
  const opts = { branch: null, runId: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--branch" && argv[i + 1]) {
      opts.branch = argv[++i];
    } else if (argv[i] === "--run-id" && argv[i + 1]) {
      opts.runId = argv[++i];
    } else if (argv[i] === "--help" || argv[i] === "-h") {
      console.log("Usage: node scripts/fetch-bin.mjs [--branch <name>] [--run-id <id>]");
      process.exit(0);
    }
  }
  return opts;
}

function getRepoSlug() {
  try {
    const url = execFileSync("git", ["remote", "get-url", "origin"], {
      cwd: ROOT,
      encoding: "utf8",
    }).trim();
    const match = url.match(/github\.com[:/]([^/]+\/[^/.]+?)(?:\.git)?$/);
    if (match) return match[1];
  } catch {
    // Fallback below
  }
  return "andereii/oxide-engine";
}

function getCurrentBranch() {
  try {
    return execFileSync("git", ["branch", "--show-current"], {
      cwd: ROOT,
      encoding: "utf8",
    }).trim() || null;
  } catch {
    return null;
  }
}

function loadEnvToken() {
  if (process.env.GITHUB_TOKEN) return process.env.GITHUB_TOKEN;
  if (process.env.GH_TOKEN) return process.env.GH_TOKEN;
  const envPath = path.join(ROOT, ".env");
  if (!fs.existsSync(envPath)) return null;
  const lines = fs.readFileSync(envPath, "utf8").split(/\r?\n/);
  for (const line of lines) {
    const m = line.match(/^\s*(?:GITHUB_TOKEN|GH_TOKEN)\s*=\s*["']?([^"'\s#]+)["']?/);
    if (m) return m[1];
  }
  return null;
}

function hasGhCli() {
  const res = spawnSync("gh", ["--version"], { stdio: "ignore" });
  return res.status === 0;
}

function downloadWithGh(repo, branch, runId) {
  let targetRunId = runId;
  if (!targetRunId) {
    const listArgs = [
      "run",
      "list",
      "--repo",
      repo,
      "--workflow",
      WORKFLOW_FILE,
      "--status",
      "success",
      "--limit",
      "1",
      "--json",
      "databaseId,headBranch,createdAt",
    ];
    if (branch) listArgs.push("--branch", branch);
    const out = execFileSync("gh", listArgs, { cwd: ROOT, encoding: "utf8" });
    const runs = JSON.parse(out);
    if (!runs.length) {
      throw new Error(`No successful runs found for ${WORKFLOW_FILE}${branch ? ` on branch ${branch}` : ""}.`);
    }
    targetRunId = String(runs[0].databaseId);
    console.log(`Found run #${targetRunId} (branch: ${runs[0].headBranch})`);
  }

  fs.mkdirSync(BIN_DIR, { recursive: true });
  console.log(`Downloading artifact '${ARTIFACT_NAME}' to bin/ ...`);
  execFileSync(
    "gh",
    ["run", "download", targetRunId, "--repo", repo, "--name", ARTIFACT_NAME, "--dir", BIN_DIR],
    { cwd: ROOT, stdio: "inherit" }
  );
}

async function ghFetch(url, token) {
  const res = await fetch(url, {
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "application/vnd.github+json",
      "User-Agent": "oxide-engine-fetch-bin",
      "X-GitHub-Api-Version": "2022-11-28",
    },
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`GitHub API ${res.status} for ${url}: ${body}`);
  }
  return res;
}

async function downloadWithToken(repo, branch, runId, token) {
  let targetRunId = runId;
  if (!targetRunId) {
    const branchQuery = branch ? `&branch=${encodeURIComponent(branch)}` : "";
    const runsUrl = `https://api.github.com/repos/${repo}/actions/workflows/${WORKFLOW_FILE}/runs?status=success&per_page=1${branchQuery}`;
    const runsData = await (await ghFetch(runsUrl, token)).json();
    const run = runsData.workflow_runs?.[0];
    if (!run) {
      throw new Error(`No successful runs found for ${WORKFLOW_FILE}${branch ? ` on branch ${branch}` : ""}.`);
    }
    targetRunId = String(run.id);
    console.log(`Found run #${targetRunId} (branch: ${run.head_branch})`);
  }

  const artifactsUrl = `https://api.github.com/repos/${repo}/actions/runs/${targetRunId}/artifacts`;
  const artifactsData = await (await ghFetch(artifactsUrl, token)).json();
  const artifact = artifactsData.artifacts?.find((a) => a.name === ARTIFACT_NAME);
  if (!artifact) {
    throw new Error(`Artifact '${ARTIFACT_NAME}' not found in run #${targetRunId}.`);
  }

  console.log(`Downloading '${ARTIFACT_NAME}' (${(artifact.size_in_bytes / 1024 / 1024).toFixed(1)} MB)...`);
  const zipRes = await ghFetch(artifact.archive_download_url, token);
  const buffer = Buffer.from(await zipRes.arrayBuffer());

  fs.mkdirSync(BIN_DIR, { recursive: true });
  const zipPath = path.join(BIN_DIR, `${ARTIFACT_NAME}.zip`);
  fs.writeFileSync(zipPath, buffer);

  console.log("Extracting into bin/ ...");
  if (process.platform === "win32") {
    execFileSync(
      "powershell",
      ["-NoProfile", "-Command", `Expand-Archive -LiteralPath '${zipPath}' -DestinationPath '${BIN_DIR}' -Force`],
      { stdio: "inherit" }
    );
  } else {
    execFileSync("unzip", ["-o", zipPath, "-d", BIN_DIR], { stdio: "inherit" });
  }
  fs.unlinkSync(zipPath);
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const repo = getRepoSlug();
  const branch = opts.branch ?? getCurrentBranch();

  if (hasGhCli()) {
    downloadWithGh(repo, branch, opts.runId);
  } else {
    const token = loadEnvToken();
    if (!token) {
      console.error("Error: Neither 'gh' CLI nor GITHUB_TOKEN/GH_TOKEN was found.");
      console.error("GitHub requires authentication to download Actions artifacts.");
      console.error("Choose one of:");
      console.error("  1. Install GitHub CLI: winget install GitHub.cli && gh auth login");
      console.error("  2. Put GITHUB_TOKEN=<your_pat_with_actions_read> in .env");
      process.exit(1);
    }
    await downloadWithToken(repo, branch, opts.runId, token);
  }

  const exePath = path.join(BIN_DIR, "oxide.exe");
  if (fs.existsSync(exePath)) {
    console.log(`\nReady! Run from project root:\n  .\\bin\\oxide.exe`);
  } else {
    console.log(`\nExtracted files to ${BIN_DIR}`);
  }
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});

