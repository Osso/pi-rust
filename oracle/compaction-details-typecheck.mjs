import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const modulePath = join(sourceLock.worktree, "packages/agent/src/harness/compaction/compaction.ts");
const tsgoPath = join(sourceLock.worktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceLock.worktree, "node_modules/@types");
const fixture = `
import type { CompactionDetails } from ${JSON.stringify(modulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
  ? [Expected] extends [Actual] ? true : false
  : false;
type Assert<Condition extends true> = Condition;
type ExpectedCompactionDetails = { readFiles: string[]; modifiedFiles: string[] };
type ExactShape = Assert<Exact<CompactionDetails, ExpectedCompactionDetails>>;
type ExactKeys = Assert<Exact<keyof CompactionDetails, keyof ExpectedCompactionDetails>>;

let details: CompactionDetails = { readFiles: [], modifiedFiles: [] };
details.readFiles = ["src/a.ts", ""];
details.modifiedFiles = ["changed\\n", "é/技能/😀\\0"];
const exactShape: ExactShape = true;
const exactKeys: ExactKeys = true;
void details;
void exactShape;
void exactKeys;
`;

const directory = mkdtempSync(join(tmpdir(), "pi-compaction-details-typecheck-"));
const fixturePath = join(directory, "compaction-details.ts");
writeFileSync(fixturePath, fixture, "utf8");
try {
  const result = spawnSync(tsgoPath, ["--noEmit", "--strict", "--skipLibCheck", "--target", "ES2022", "--module", "NodeNext", "--moduleResolution", "NodeNext", "--allowImportingTsExtensions", "--types", "node", "--typeRoots", typeRoots, fixturePath], { cwd: repoRoot, encoding: "utf8" });
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  process.exitCode = result.status ?? 1;
} finally {
  rmSync(directory, { recursive: true, force: true });
}
