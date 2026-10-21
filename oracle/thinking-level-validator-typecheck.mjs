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
const argsModulePath = join(sourceWorktree, "packages/coding-agent/src/cli/args.ts");
const thinkingLevelModulePath = join(sourceWorktree, "packages/agent-core/src/types.ts");
const highlightDeclarationPath = join(
  sourceWorktree,
  "packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const tsxPath = join(sourceLock.runtimeWorktree, "node_modules/.bin/tsx");

const fixtureSource = `
import { isValidThinkingLevel } from ${JSON.stringify(argsModulePath)};
import type { ThinkingLevel } from ${JSON.stringify(thinkingLevelModulePath)};
// @ts-expect-error VALID_THINKING_LEVELS is private and not exported.
import { VALID_THINKING_LEVELS } from ${JSON.stringify(argsModulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
  ? [Expected] extends [Actual] ? true : false
  : false;
type Assert<Condition extends true> = Condition;
type ExpectedPredicate = (level: string) => level is ThinkingLevel;
type PredicateIsExact = Assert<Exact<typeof isValidThinkingLevel, ExpectedPredicate>>;

const validValues = ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"] as const;
const invalidValues = [
  "",
  "OFF",
  "Medium",
  " medium",
  "medium ",
  "x-high",
  "extreme",
  "unsupported",
  "é😀",
  "\\u0000",
  "\\n",
] as const;

let candidate: string = "medium";
if (isValidThinkingLevel(candidate)) {
  const narrowed: ThinkingLevel = candidate;
  type NarrowedCandidateIsExact = Assert<Exact<typeof candidate, ThinkingLevel>>;
  const narrowedCandidateIsExact: NarrowedCandidateIsExact = true;
  void narrowed;
  void narrowedCandidateIsExact;
}

for (const value of validValues) {
  if (!isValidThinkingLevel(value)) {
    throw new Error("valid thinking level rejected: " + value);
  }
}
for (const value of invalidValues) {
  if (isValidThinkingLevel(value)) {
    throw new Error("invalid thinking level accepted: " + value);
  }
}

const predicateIsExact: PredicateIsExact = true;
void VALID_THINKING_LEVELS;
void predicateIsExact;
`;

function validateSourceLock() {
  if (typeof sourceWorktree !== "string" || sourceWorktree.length === 0) {
    throw new Error("source lock worktree must be a non-empty string");
  }
  if (typeof sourceLock.commit !== "string" || !/^[0-9a-f]{40}$/.test(sourceLock.commit)) {
    throw new Error("source lock commit must be a 40-character hexadecimal string");
  }
}

function typecheckArguments(fixturePath) {
  const configPath = join(dirname(fixturePath), "tsconfig.json");
  writeFileSync(
    configPath,
    `${JSON.stringify(buildSourceTsconfig(sourceWorktree, [fixturePath, highlightDeclarationPath]), null, 2)}\n`,
    "utf8",
  );
  return ["-p", configPath];
}

function runProcess(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8" });
  if (result.stdout !== "") process.stdout.write(result.stdout);
  if (result.stderr !== "") process.stderr.write(result.stderr);
  if (result.error !== undefined) throw new Error(`${command} failed to start: ${result.error.message}`);
  return result.status ?? 1;
}

function main() {
  validateSourceLock();
  const directory = mkdtempSync(join(tmpdir(), "pi-thinking-level-validator-typecheck-"));
  const fixturePath = join(directory, "thinking-level-validator.ts");
  writeFileSync(fixturePath, fixtureSource, "utf8");
  try {
    const typecheckStatus = runCompiler(tsgoPath, typecheckArguments(fixturePath), repoRoot);
    if (typecheckStatus !== 0) return typecheckStatus;
    return runProcess(tsxPath, ["--tsconfig", join(sourceLock.runtimeWorktree, "tsconfig.json"), prepareRuntimeFixture(fixturePath, sourceLock)]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

try {
  process.exitCode = main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`thinking-level validator typeproof failed: ${message}\n`);
  process.exitCode = 1;
}
