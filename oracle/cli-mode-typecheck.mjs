import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const sourceWorktree = sourceLock.worktree;
const modeModulePath = join(sourceWorktree, "packages/coding-agent/src/cli/args.ts");
const highlightDeclarationPath = join(
  sourceWorktree,
  "packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceWorktree, "node_modules/@types");
const fixtureSource = `
import type { Mode } from ${JSON.stringify(modeModulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
  ? [Expected] extends [Actual] ? true : false
  : false;
type Assert<Condition extends true> = Condition;
type ExpectedMode = "text" | "json" | "rpc";
type ExactMode = Assert<Exact<Mode, ExpectedMode>>;

const text: Mode = "text";
const json: Mode = "json";
const rpc: Mode = "rpc";
// @ts-expect-error empty literal is not a CLI mode.
const empty: Mode = "";
// @ts-expect-error TUI is not a CLI mode.
const tui: Mode = "tui";
// @ts-expect-error uppercase RPC is not a CLI mode.
const uppercaseRpc: Mode = "RPC";

const exactMode: ExactMode = true;
void text;
void json;
void rpc;
void empty;
void tui;
void uppercaseRpc;
void exactMode;
`;

function validateSourceLock() {
  if (typeof sourceWorktree !== "string" || sourceWorktree.length === 0) {
    throw new Error("source lock worktree must be a non-empty string");
  }
  if (typeof sourceLock.commit !== "string" || !/^[0-9a-f]{40}$/.test(sourceLock.commit)) {
    throw new Error("source lock commit must be a 40-character hexadecimal string");
  }
}

function verifyPinnedSourceHead() {
  const result = spawnSync("git", ["-C", sourceWorktree, "rev-parse", "HEAD"], { encoding: "utf8" });
  if (result.error !== undefined) throw new Error(`source HEAD check failed to start git: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`source HEAD check failed: ${result.stderr.trim()}`);
  const actualCommit = result.stdout.trim();
  if (actualCommit !== sourceLock.commit) {
    throw new Error(`pinned TypeScript source HEAD mismatch: expected ${sourceLock.commit}, got ${actualCommit}`);
  }
}

function typecheckArguments(fixturePath) {
  return [
    "--noEmit",
    "--strict",
    "--skipLibCheck",
    "--target",
    "ES2022",
    "--module",
    "NodeNext",
    "--moduleResolution",
    "NodeNext",
    "--allowImportingTsExtensions",
    "--types",
    "node",
    "--typeRoots",
    typeRoots,
    fixturePath,
    highlightDeclarationPath,
  ];
}

function runTypecheck(fixturePath) {
  const result = spawnSync(tsgoPath, typecheckArguments(fixturePath), {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  return result.status ?? 1;
}

function main() {
  validateSourceLock();
  verifyPinnedSourceHead();
  const directory = mkdtempSync(join(tmpdir(), "pi-cli-mode-typecheck-"));
  const fixturePath = join(directory, "cli-mode.ts");
  writeFileSync(fixturePath, fixtureSource, "utf8");
  try {
    return runTypecheck(fixturePath);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

try {
  process.exitCode = main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`CLI mode type proof failed: ${message}\n`);
  process.exitCode = 1;
}
