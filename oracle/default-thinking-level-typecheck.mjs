import { selectTypecheckReference, prepareRuntimeFixture } from "./typecheck-reference.mjs";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";
import { runCompiler } from "./typecheck-report.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = selectTypecheckReference(repoRoot);
const sourceWorktree = sourceLock.worktree;
const defaultsModulePath = join(sourceWorktree, "packages/coding-agent/src/core/defaults.ts");
const thinkingLevelModulePath = join(sourceWorktree, "packages/agent-core/src/types.ts");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const tsxPath = join(sourceLock.runtimeWorktree, "node_modules/.bin/tsx");

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

function runProcess(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8" });
  if (result.stdout !== "") process.stdout.write(result.stdout);
  if (result.stderr !== "") process.stderr.write(result.stderr);
  if (result.error !== undefined) throw new Error(`${command} failed to start: ${result.error.message}`);
  return result.status ?? 1;
}

function runTypecheck(fixturePath) {
  const configPath = join(dirname(fixturePath), "tsconfig.json");
  writeFileSync(configPath, `${JSON.stringify(buildSourceTsconfig(sourceWorktree, [fixturePath]), null, 2)}\n`, "utf8");
  return runCompiler(tsgoPath, ["-p", configPath], repoRoot);
}

function runRuntimeValueProof(fixturePath) {
  return runProcess(tsxPath, ["--tsconfig", join(sourceLock.runtimeWorktree, "tsconfig.json"), prepareRuntimeFixture(fixturePath, sourceLock)]);
}

function main() {
  validateSourceLock();
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
