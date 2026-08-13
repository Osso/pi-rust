import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLockPath = join(repoRoot, "docs/porting/source-lock.json");
const sourceLock = JSON.parse(readFileSync(sourceLockPath, "utf8"));
const skillBlockModulePath = join(sourceLock.worktree, "packages/coding-agent/src/core/skill-block.ts");
const tsgoPath = join(sourceLock.worktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceLock.worktree, "node_modules/@types");

const fixtureSource = `
import { parseSkillBlock, type ParsedSkillBlock } from ${JSON.stringify(skillBlockModulePath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
	? [Expected] extends [Actual]
		? true
		: false
	: false;
type Assert<Condition extends true> = Condition;

type ExpectedParsedSkillBlock = {
	name: string;
	location: string;
	content: string;
	userMessage: string | undefined;
};
type ParsedSkillBlockIsExact = Assert<Exact<ParsedSkillBlock, ExpectedParsedSkillBlock>>;
type ParserResultIsExact = Assert<Exact<ReturnType<typeof parseSkillBlock>, ParsedSkillBlock | null>>;

let block: ParsedSkillBlock = {
	name: "name",
	location: "location",
	content: "content",
	userMessage: undefined,
};
block.name = "updated name";
block.location = "updated location";
block.content = "updated content";
block.userMessage = "updated message";
block.userMessage = undefined;

const parsed: ParsedSkillBlock | null = parseSkillBlock("<skill name=\\"n\\" location=\\"l\\">\\n\\nx\\n</skill>");
void block;
void parsed;
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
	const temporaryDirectory = mkdtempSync(join(tmpdir(), "pi-skill-block-typecheck-"));
	const fixturePath = join(temporaryDirectory, "skill-block.ts");
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
	process.stderr.write(`skill block typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
