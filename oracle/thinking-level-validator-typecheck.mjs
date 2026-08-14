import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const sourceWorktree = sourceLock.worktree;
const argsModulePath = join(sourceWorktree, "packages/coding-agent/src/cli/args.ts");
const thinkingLevelModulePath = join(sourceWorktree, "packages/agent/src/types.ts");
const highlightDeclarationPath = join(
  sourceWorktree,
  "packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const tsxPath = join(sourceWorktree, "node_modules/.bin/tsx");
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const typeRoots = join(sourceWorktree, "node_modules/@types");

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
    "--resolveJsonModule",
    "--types",
    "node",
    "--typeRoots",
    typeRoots,
    fixturePath,
    highlightDeclarationPath,
  ];
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
  verifyPinnedSourceHead();
  const directory = mkdtempSync(join(tmpdir(), "pi-thinking-level-validator-typecheck-"));
  const fixturePath = join(directory, "thinking-level-validator.ts");
  writeFileSync(fixturePath, fixtureSource, "utf8");
  try {
    const typecheckStatus = runProcess(tsgoPath, typecheckArguments(fixturePath));
    if (typecheckStatus !== 0) return typecheckStatus;
    return runProcess(tsxPath, ["--tsconfig", tsconfigPath, fixturePath]);
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
