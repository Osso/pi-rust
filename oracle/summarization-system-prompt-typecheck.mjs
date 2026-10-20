import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";
import { runCompiler } from "./typecheck-report.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLockPath = join(repoRoot, "docs/porting/source-lock.json");
const sourceLock = JSON.parse(readFileSync(sourceLockPath, "utf8"));
const sourceWorktree = sourceLock.worktree;
const compactionModulePath = join(sourceWorktree, "packages/agent/src/harness/compaction/compaction.ts");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const EXPECTED_PROMPT =
	"You are a context summarization assistant. Your task is to read a conversation between a user and an AI assistant, then produce a structured summary following the exact format specified.\n\nDo NOT continue the conversation. Do NOT respond to any questions in the conversation. ONLY output the structured summary.";

const fixtureSource = `
import { SUMMARIZATION_SYSTEM_PROMPT } from ${JSON.stringify(compactionModulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
	? [Expected] extends [Actual]
		? true
		: false
	: false;
type Assert<Condition extends true> = Condition;
type ExpectedPrompt = ${JSON.stringify(EXPECTED_PROMPT)};
type SummarizationSystemPromptIsExact = Assert<
	Exact<typeof SUMMARIZATION_SYSTEM_PROMPT, ExpectedPrompt>
>;

void SUMMARIZATION_SYSTEM_PROMPT;
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
	if (EXPECTED_PROMPT.length !== 310) {
		throw new Error(`expected prompt length mismatch: ${EXPECTED_PROMPT.length}`);
	}
}

function verifySourceHead() {
	const result = spawnSync("git", ["-C", sourceWorktree, "rev-parse", "HEAD"], {
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

function runTypecheck(fixturePath) {
	const configPath = join(dirname(fixturePath), "tsconfig.json");
	writeFileSync(configPath, `${JSON.stringify(buildSourceTsconfig(sourceWorktree, [fixturePath]), null, 2)}\n`, "utf8");
	return runCompiler(tsgoPath, ["-p", configPath], repoRoot);
}

function typecheckFixture() {
	const temporaryDirectory = mkdtempSync(join(tmpdir(), "pi-summarization-system-prompt-typecheck-"));
	const fixturePath = join(temporaryDirectory, "summarization-system-prompt.ts");
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
	process.stderr.write(`summarization system prompt typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
