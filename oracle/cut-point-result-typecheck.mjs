import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLockPath = join(repoRoot, "docs/porting/source-lock.json");
const sourceLock = JSON.parse(readFileSync(sourceLockPath, "utf8"));
const compactionModulePath = join(sourceLock.worktree, "packages/agent/src/harness/compaction/compaction.ts");
const tsgoPath = join(sourceLock.worktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceLock.worktree, "node_modules/@types");

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

function verifySourceHead() {
	const result = spawnSync("git", ["-C", sourceLock.worktree, "rev-parse", "HEAD"], {
		encoding: "utf8",
	});
	if (result.error !== undefined) {
		throw new Error(`source HEAD check failed to start git: ${result.error.message}`);
	}
	if (result.status !== 0) {
		throw new Error(`source HEAD check failed: ${result.stderr.trim()}`);
	}
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
	];
}

function runTypecheck(fixturePath) {
	const result = spawnSync(tsgoPath, typecheckArguments(fixturePath), {
		cwd: repoRoot,
		encoding: "utf8",
	});
	if (result.stdout !== "") process.stdout.write(result.stdout);
	if (result.stderr !== "") process.stderr.write(result.stderr);
	if (result.error !== undefined) {
		throw new Error(`pinned tsgo failed to start: ${result.error.message}`);
	}
	return result.status ?? 1;
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
	verifySourceHead();
	process.exitCode = typecheckFixture();
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`cut point result typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
