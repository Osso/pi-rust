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
import type { ContextUsageEstimate } from ${JSON.stringify(compactionModulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
	? [Expected] extends [Actual]
		? true
		: false
	: false;
type Assert<Condition extends true> = Condition;
type ExpectedContextUsageEstimate = {
	tokens: number;
	usageTokens: number;
	trailingTokens: number;
	lastUsageIndex: number | null;
};
type ContextUsageEstimateIsExact = Assert<Exact<ContextUsageEstimate, ExpectedContextUsageEstimate>>;
type ContextUsageEstimateKeysAreExact = Assert<Exact<keyof ContextUsageEstimate, keyof ExpectedContextUsageEstimate>>;

let estimate: ContextUsageEstimate = {
	tokens: 42.5,
	usageTokens: 17.25,
	trailingTokens: 3.75,
	lastUsageIndex: null,
};
estimate.tokens = -0;
estimate.tokens = Number.NaN;
estimate.tokens = Number.POSITIVE_INFINITY;
estimate.tokens = Number.NEGATIVE_INFINITY;
estimate.tokens = Number.MAX_VALUE;
estimate.tokens = Number.MIN_VALUE;
estimate.usageTokens = -0;
estimate.usageTokens = Number.NaN;
estimate.usageTokens = Number.POSITIVE_INFINITY;
estimate.usageTokens = Number.NEGATIVE_INFINITY;
estimate.usageTokens = Number.MAX_VALUE;
estimate.usageTokens = Number.MIN_VALUE;
estimate.trailingTokens = -0;
estimate.trailingTokens = Number.NaN;
estimate.trailingTokens = Number.POSITIVE_INFINITY;
estimate.trailingTokens = Number.NEGATIVE_INFINITY;
estimate.trailingTokens = Number.MAX_VALUE;
estimate.trailingTokens = Number.MIN_VALUE;
estimate.lastUsageIndex = null;
estimate.lastUsageIndex = -2.5;
estimate.lastUsageIndex = -0;
estimate.lastUsageIndex = Number.NaN;
estimate.lastUsageIndex = Number.POSITIVE_INFINITY;
estimate.lastUsageIndex = Number.NEGATIVE_INFINITY;
estimate.lastUsageIndex = Number.MAX_VALUE;
estimate.lastUsageIndex = Number.MIN_VALUE;
void estimate;
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
	const temporaryDirectory = mkdtempSync(join(tmpdir(), "pi-context-usage-estimate-typecheck-"));
	const fixturePath = join(temporaryDirectory, "context-usage-estimate.ts");
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
	process.stderr.write(`context usage estimate typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
