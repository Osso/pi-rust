import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { runCompiler } from "./typecheck-report.mjs";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const modulePath = join(sourceLock.worktree, "packages/agent/src/harness/compaction/compaction.ts");
const tsgoPath = join(sourceLock.worktree, "node_modules/.bin/tsgo");
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
  const configPath = join(dirname(fixturePath), "tsconfig.json");
  writeFileSync(configPath, `${JSON.stringify(buildSourceTsconfig(sourceLock.worktree, [fixturePath]), null, 2)}\n`, "utf8");
  process.exitCode = runCompiler(tsgoPath, ["-p", configPath], repoRoot);
} finally {
  rmSync(directory, { recursive: true, force: true });
}
