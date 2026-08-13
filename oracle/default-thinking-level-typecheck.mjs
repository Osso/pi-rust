import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const sourceWorktree = sourceLock.worktree;
const defaultsModulePath = join(sourceWorktree, "packages/coding-agent/src/core/defaults.ts");
const thinkingLevelModulePath = join(sourceWorktree, "packages/agent/src/types.ts");
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const tsxPath = join(sourceWorktree, "node_modules/.bin/tsx");
const typeRoots = join(sourceWorktree, "node_modules/@types");

const fixtureSource = `
import { DEFAULT_THINKING_LEVEL } from ${JSON.stringify(defaultsModulePath)};
import type { ThinkingLevel } from ${JSON.stringify(thinkingLevelModulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
  ? [Expected] extends [Actual]
    ? true
    : false
  : false;
type Assert<Condition extends true> = Condition;
type ExportTypeIsAgentThinkingLevel = Assert<Exact<typeof DEFAULT_THINKING_LEVEL, ThinkingLevel>>;
const typedValue: ThinkingLevel = DEFAULT_THINKING_LEVEL;
if (DEFAULT_THINKING_LEVEL !== "medium") {
  throw new Error("DEFAULT_THINKING_LEVEL must equal medium, got " + DEFAULT_THINKING_LEVEL);
}
void typedValue;
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

function runProcess(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8" });
  if (result.stdout !== "") process.stdout.write(result.stdout);
  if (result.stderr !== "") process.stderr.write(result.stderr);
  if (result.error !== undefined) throw new Error(`${command} failed to start: ${result.error.message}`);
  return result.status ?? 1;
}

function runTypecheck(fixturePath) {
  return runProcess(tsgoPath, [
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
  ]);
}

function runRuntimeValueProof(fixturePath) {
  return runProcess(tsxPath, ["--tsconfig", tsconfigPath, fixturePath]);
}

function main() {
  validateSourceLock();
  verifyPinnedSourceHead();
  const directory = mkdtempSync(join(tmpdir(), "pi-default-thinking-level-typecheck-"));
  const fixturePath = join(directory, "default-thinking-level.ts");
  writeFileSync(fixturePath, fixtureSource, "utf8");
  try {
    const typecheckStatus = runTypecheck(fixturePath);
    if (typecheckStatus !== 0) return typecheckStatus;
    return runRuntimeValueProof(fixturePath);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

try {
  process.exitCode = main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`default thinking level type/value proof failed: ${message}\n`);
  process.exitCode = 1;
}
