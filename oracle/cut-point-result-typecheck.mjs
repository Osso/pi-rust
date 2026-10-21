import { selectTypecheckReference } from "./typecheck-reference.mjs";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";
import { runCompiler } from "./typecheck-report.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = selectTypecheckReference(repoRoot);
const compactionModulePath = join(sourceLock.worktree, "packages/agent-core/src/harness/compaction/compaction.ts");
const tsgoPath = join(sourceLock.worktree, "node_modules/.bin/tsgo");

const fixtureSource = `
import type { CutPointResult } from ${JSON.stringify(compactionModulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
	? [Expected] extends [Actual]
		? true
		: false
	: false;
type Assert<Condition extends true> = Condition;
type ExpectedCutPointResult = {
	firstKeptEntryIndex: number;
	turnStartIndex: number;
	isSplitTurn: boolean;
};
type CutPointResultIsExact = Assert<Exact<CutPointResult, ExpectedCutPointResult>>;

let result: CutPointResult = {
	firstKeptEntryIndex: 0,
	turnStartIndex: -1,
	isSplitTurn: false,
};
result.firstKeptEntryIndex = -1;
result.firstKeptEntryIndex = -0;
result.firstKeptEntryIndex = Number.NaN;
result.firstKeptEntryIndex = Number.POSITIVE_INFINITY;
result.firstKeptEntryIndex = Number.NEGATIVE_INFINITY;
result.firstKeptEntryIndex = Number.MAX_VALUE;
result.firstKeptEntryIndex = Number.MIN_VALUE;
result.turnStartIndex = -1;
result.turnStartIndex = -0;
result.turnStartIndex = Number.NaN;
result.turnStartIndex = Number.POSITIVE_INFINITY;
result.turnStartIndex = Number.NEGATIVE_INFINITY;
result.turnStartIndex = Number.MAX_VALUE;
result.turnStartIndex = Number.MIN_VALUE;
result.isSplitTurn = true;
result.isSplitTurn = false;
void result;
`;

function validateSourceLock() {
	if (typeof sourceLock.worktree !== "string" || sourceLock.worktree.length === 0) {
		throw new Error("source lock worktree must be a non-empty string");
	}
	if (typeof sourceLock.commit !== "string" || !/^[0-9a-f]{40}$/.test(sourceLock.commit)) {
		throw new Error("source lock commit must be a 40-character hexadecimal string");
	}
	if (sourceLock.sourcePackage !== "@earendil-works/pi-protocol") {
		throw new Error("source lock package is not the pinned Pi protocol package");
	}
	if (typeof sourceLock.sourceVersion !== "string" || sourceLock.sourceVersion.length === 0) {
		throw new Error("source lock version must be a non-empty string");
	}
	if (typeof sourceLock.lockedOn !== "string" || sourceLock.lockedOn.length === 0) {
		throw new Error("source lock date must be a non-empty string");
	}
}

function runTypecheck(fixturePath) {
	const configPath = join(dirname(fixturePath), "tsconfig.json");
	writeFileSync(configPath, `${JSON.stringify(buildSourceTsconfig(sourceLock.worktree, [fixturePath]), null, 2)}\n`, "utf8");
	return runCompiler(tsgoPath, ["-p", configPath], repoRoot);
}

function typecheckFixture() {
	const temporaryDirectory = mkdtempSync(join(tmpdir(), "pi-cut-point-result-typecheck-"));
	const fixturePath = join(temporaryDirectory, "cut-point-result.ts");
	writeFileSync(fixturePath, fixtureSource, "utf8");
	try {
		return runTypecheck(fixturePath);
	} finally {
		rmSync(temporaryDirectory, { recursive: true, force: true });
	}
}

try {
	validateSourceLock();
	process.exitCode = typecheckFixture();
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`cut point result typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
