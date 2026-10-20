import { selectTypecheckReference } from "./typecheck-reference.mjs";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";
import { runCompiler } from "./typecheck-report.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = selectTypecheckReference(repoRoot);
const skillBlockModulePath = join(sourceLock.worktree, "packages/coding-agent/src/core/skill-block.ts");
const tsgoPath = join(sourceLock.worktree, "node_modules/.bin/tsgo");

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

function runTypecheck(fixturePath) {
	const configPath = join(dirname(fixturePath), "tsconfig.json");
	writeFileSync(configPath, `${JSON.stringify(buildSourceTsconfig(sourceLock.worktree, [fixturePath]), null, 2)}\n`, "utf8");
	return runCompiler(tsgoPath, ["-p", configPath], repoRoot);
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
	process.exitCode = typecheckFixture();
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`skill block typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
