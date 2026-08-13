import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLockPath = join(repoRoot, "docs/porting/source-lock.json");
const sourceLock = JSON.parse(readFileSync(sourceLockPath, "utf8"));
const sourceWorktree = sourceLock.worktree;
const publicIndexPath = join(sourceWorktree, "packages/agent/src/index.ts");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceWorktree, "node_modules/@types");

const fixtureSource = `
import {
	BRANCH_SUMMARY_PREFIX,
	BRANCH_SUMMARY_SUFFIX,
	COMPACTION_SUMMARY_PREFIX,
	COMPACTION_SUMMARY_SUFFIX,
} from ${JSON.stringify(publicIndexPath)};
import type {
	AgentHarnessResources,
	BashExecutionMessage,
	BranchSummaryMessage,
	FileInfo,
	FileKind,
	PromptTemplate,
	Skill,
} from ${JSON.stringify(publicIndexPath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
	? [Expected] extends [Actual]
		? true
		: false
	: false;
type Assert<Condition extends true> = Condition;

type ExpectedCompactionSummaryPrefix =
	"The conversation history before this point was compacted into the following summary:\\n\\n<summary>\\n";
type ExpectedCompactionSummarySuffix = "\\n</summary>";
type ExpectedBranchSummaryPrefix =
	"The following is a summary of a branch that this conversation came back from:\\n\\n<summary>\\n";
type ExpectedBranchSummarySuffix = "</summary>";
type CompactionSummaryPrefixIsExact = Assert<
	Exact<typeof COMPACTION_SUMMARY_PREFIX, ExpectedCompactionSummaryPrefix>
>;
type CompactionSummarySuffixIsExact = Assert<
	Exact<typeof COMPACTION_SUMMARY_SUFFIX, ExpectedCompactionSummarySuffix>
>;
type BranchSummaryPrefixIsExact = Assert<Exact<typeof BRANCH_SUMMARY_PREFIX, ExpectedBranchSummaryPrefix>>;
type BranchSummarySuffixIsExact = Assert<Exact<typeof BRANCH_SUMMARY_SUFFIX, ExpectedBranchSummarySuffix>>;

type ExpectedBashExecutionMessage = {
	role: "bashExecution";
	command: string;
	output: string;
	exitCode: number | undefined;
	cancelled: boolean;
	truncated: boolean;
	fullOutputPath?: string;
	timestamp: number;
	excludeFromContext?: boolean;
};
type BashExecutionMessageIsExact = Assert<Exact<BashExecutionMessage, ExpectedBashExecutionMessage>>;

type ExpectedBranchSummaryMessage = {
	role: "branchSummary";
	summary: string;
	fromId: string;
	timestamp: number;
};
type BranchSummaryMessageIsExact = Assert<Exact<BranchSummaryMessage, ExpectedBranchSummaryMessage>>;

type ExpectedFileKind = "file" | "directory" | "symlink";
type ExpectedFileInfo = {
	name: string;
	path: string;
	kind: ExpectedFileKind;
	size: number;
	mtimeMs: number;
};
type ExpectedSkill = {
	name: string;
	description: string;
	content: string;
	filePath: string;
	disableModelInvocation?: boolean;
};
type ExpectedPromptTemplate = {
	name: string;
	description?: string;
	content: string;
};
type ExpectedResources = {
	promptTemplates?: PromptTemplate[];
	skills?: Skill[];
};

type FileKindIsExact = Assert<Exact<FileKind, ExpectedFileKind>>;
type FileInfoIsExact = Assert<Exact<FileInfo, ExpectedFileInfo>>;
type SkillIsExact = Assert<Exact<Skill, ExpectedSkill>>;
type PromptTemplateIsExact = Assert<Exact<PromptTemplate, ExpectedPromptTemplate>>;
type ResourcesIsExact = Assert<Exact<AgentHarnessResources, ExpectedResources>>;

let bashMessage: BashExecutionMessage = {
	role: "bashExecution",
	command: "printf test",
	output: "test",
	exitCode: 0,
	cancelled: false,
	truncated: false,
	timestamp: 0,
};
bashMessage.role = "bashExecution";
bashMessage.command = "updated";
bashMessage.output = "updated output";
bashMessage.exitCode = -0;
bashMessage.exitCode = Number.NaN;
bashMessage.exitCode = Number.POSITIVE_INFINITY;
bashMessage.exitCode = Number.NEGATIVE_INFINITY;
bashMessage.exitCode = undefined;
bashMessage.cancelled = true;
bashMessage.truncated = true;
bashMessage.fullOutputPath = "/tmp/output.log";
bashMessage.fullOutputPath = undefined;
bashMessage.timestamp = -0;
bashMessage.timestamp = Number.NaN;
bashMessage.timestamp = Number.POSITIVE_INFINITY;
bashMessage.timestamp = Number.NEGATIVE_INFINITY;
bashMessage.excludeFromContext = false;
bashMessage.excludeFromContext = true;
bashMessage.excludeFromContext = undefined;

const bashMessageWithExplicitUndefined: BashExecutionMessage = {
	role: "bashExecution",
	command: "",
	output: "",
	exitCode: undefined,
	cancelled: false,
	truncated: false,
	fullOutputPath: undefined,
	timestamp: -0,
	excludeFromContext: undefined,
};

let branchSummaryMessage: BranchSummaryMessage = {
	role: "branchSummary",
	summary: "summary",
	fromId: "branch-id",
	timestamp: 0,
};
branchSummaryMessage.role = "branchSummary";
branchSummaryMessage.summary = "updated summary";
branchSummaryMessage.fromId = "updated-branch-id";
branchSummaryMessage.timestamp = -0;
branchSummaryMessage.timestamp = Number.NaN;
branchSummaryMessage.timestamp = Number.POSITIVE_INFINITY;
branchSummaryMessage.timestamp = Number.NEGATIVE_INFINITY;

let info: FileInfo = {
	name: "name",
	path: "/path",
	kind: "file",
	size: 0,
	mtimeMs: 0,
};
info.name = "updated";
info.path = "updated/path";
info.kind = "directory";
info.size = -0;
info.size = Number.NaN;
info.size = Number.POSITIVE_INFINITY;
info.size = Number.NEGATIVE_INFINITY;
info.mtimeMs = -0;
info.mtimeMs = Number.NaN;
info.mtimeMs = Number.POSITIVE_INFINITY;
info.mtimeMs = Number.NEGATIVE_INFINITY;

let skill: Skill = {
	name: "skill",
	description: "description",
	content: "content",
	filePath: "/skills/skill/SKILL.md",
};
skill.name = "updated-skill";
skill.description = "updated description";
skill.content = "updated content";
skill.filePath = "/skills/updated/SKILL.md";
skill.disableModelInvocation = false;
skill.disableModelInvocation = true;
skill.disableModelInvocation = undefined;
const skillWithUndefined: Skill = {
	name: "undefined-skill",
	description: "description",
	content: "content",
	filePath: "/skills/undefined/SKILL.md",
	disableModelInvocation: undefined,
};

let promptTemplate: PromptTemplate = {
	name: "template",
	content: "content",
};
promptTemplate.name = "updated-template";
promptTemplate.description = "description";
promptTemplate.description = "";
promptTemplate.description = undefined;
promptTemplate.content = "updated content";
const promptTemplateWithUndefined: PromptTemplate = {
	name: "undefined-template",
	description: undefined,
	content: "content",
};

let resources: AgentHarnessResources = {};
resources.promptTemplates = [];
resources.skills = [];
resources.promptTemplates = [promptTemplate];
resources.skills = [skill, skillWithUndefined];
resources.promptTemplates[0].name = "mutated-template";
resources.skills[0].disableModelInvocation = false;

const customSkill = { ...skill, applicationId: 1 };
const customPromptTemplate = { ...promptTemplate, command: "/custom" };
type CustomSkill = typeof customSkill;
type CustomPromptTemplate = typeof customPromptTemplate;
type CustomResources = AgentHarnessResources<CustomSkill, CustomPromptTemplate>;
type ExpectedCustomResources = {
	promptTemplates?: CustomPromptTemplate[];
	skills?: CustomSkill[];
};
type CustomResourcesAreExact = Assert<Exact<CustomResources, ExpectedCustomResources>>;

let customResources: CustomResources = {
	promptTemplates: [customPromptTemplate],
	skills: [customSkill],
};
customResources.promptTemplates = [];
customResources.skills = [];
customResources.promptTemplates = [customPromptTemplate];
customResources.skills = [customSkill];
customResources.skills[0].applicationId = 2;
customResources.promptTemplates[0].command = "/updated";

void bashMessage;
void bashMessageWithExplicitUndefined;
void branchSummaryMessage;
void info;
void resources;
void customResources;
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
		throw new Error(
			`pinned TypeScript source HEAD mismatch: expected ${sourceLock.commit}, got ${actualCommit}`,
		);
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
		"--resolveJsonModule",
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
	const temporaryDirectory = mkdtempSync(join(tmpdir(), "pi-public-index-typecheck-"));
	const fixturePath = join(temporaryDirectory, "public-index-types.ts");
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
	process.stderr.write(`public index typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
