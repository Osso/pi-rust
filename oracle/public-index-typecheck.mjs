import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLockPath = join(repoRoot, "docs/porting/source-lock.json");
const sourceLock = JSON.parse(readFileSync(sourceLockPath, "utf8"));
const sourceWorktree = sourceLock.worktree;
// This fixture imports the pinned agent package index, not the protocol package schema.
const publicIndexPath = join(sourceWorktree, "packages/agent/src/index.ts");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceWorktree, "node_modules/@types");

const fixtureSource = `
import {
	BRANCH_SUMMARY_PREFIX,
	BRANCH_SUMMARY_SUFFIX,
	COMPACTION_SUMMARY_PREFIX,
	COMPACTION_SUMMARY_SUFFIX,
	shouldCompact,
} from ${JSON.stringify(publicIndexPath)};
import type {
	AgentHarnessResources,
	BashExecutionMessage,
	BranchSummaryMessage,
	CompactionSettings,
	FileInfo,
	FileKind,
	PromptTemplate,
	PromptTemplateDiagnostic,
	PromptTemplateDiagnosticCode,
	QueueMode,
	ToolExecutionMode,
	ThinkingLevel as AgentThinkingLevel,
	AgentToolExecutionContext,
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

type ExpectedCompactionSettings = {
	enabled: boolean;
	reserveTokens: number;
	keepRecentTokens: number;
};
type CompactionSettingsIsExact = Assert<Exact<CompactionSettings, ExpectedCompactionSettings>>;

type ExpectedQueueMode = "all" | "one-at-a-time";
type QueueModeIsExact = Assert<Exact<QueueMode, ExpectedQueueMode>>;

type ExpectedToolExecutionMode = "sequential" | "parallel";
type ToolExecutionModeIsExact = Assert<Exact<ToolExecutionMode, ExpectedToolExecutionMode>>;

type ExpectedAgentThinkingLevel =
	| "off"
	| "minimal"
	| "low"
	| "medium"
	| "high"
	| "xhigh"
	| "max"
	| "ultra";
type AgentThinkingLevelIsExact = Assert<Exact<AgentThinkingLevel, ExpectedAgentThinkingLevel>>;

type ExpectedAgentToolExecutionContext = {
	startedAt: number;
	agentId?: string;
};
type AgentToolExecutionContextIsExact = Assert<
	Exact<AgentToolExecutionContext, ExpectedAgentToolExecutionContext>
>;
type AgentToolExecutionContextStartedAtIsNumber = Assert<
	Exact<AgentToolExecutionContext["startedAt"], number>
>;
type AgentToolExecutionContextAgentIdIsOptionalString = Assert<
	Exact<AgentToolExecutionContext["agentId"], string | undefined>
>;

type ExpectedPromptTemplateDiagnosticCode = "file_info_failed" | "list_failed" | "read_failed" | "parse_failed";
type PromptTemplateDiagnosticCodeIsExact = Assert<
	Exact<PromptTemplateDiagnosticCode, ExpectedPromptTemplateDiagnosticCode>
>;
type ExpectedPromptTemplateDiagnostic = {
	type: "warning";
	code: PromptTemplateDiagnosticCode;
	message: string;
	path: string;
};
type PromptTemplateDiagnosticIsExact = Assert<
	Exact<PromptTemplateDiagnostic, ExpectedPromptTemplateDiagnostic>
>;

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

let compactionSettings: CompactionSettings = {
	enabled: true,
	reserveTokens: 10,
	keepRecentTokens: 20,
};
compactionSettings.enabled = false;
compactionSettings.enabled = true;
compactionSettings.reserveTokens = -0;
compactionSettings.reserveTokens = Number.NaN;
compactionSettings.reserveTokens = Number.POSITIVE_INFINITY;
compactionSettings.reserveTokens = Number.NEGATIVE_INFINITY;
compactionSettings.keepRecentTokens = -0;
compactionSettings.keepRecentTokens = Number.NaN;
compactionSettings.keepRecentTokens = Number.POSITIVE_INFINITY;
compactionSettings.keepRecentTokens = Number.NEGATIVE_INFINITY;
const compactionDecision = shouldCompact(Number.MAX_VALUE, -Number.MAX_VALUE, compactionSettings);

let queueMode: QueueMode = "all";
queueMode = "one-at-a-time";
// @ts-expect-error QueueMode excludes the empty literal.
queueMode = "";
// @ts-expect-error QueueMode excludes the alternate spelling.
queueMode = "one_at_a_time";

let toolExecutionMode: ToolExecutionMode = "sequential";
toolExecutionMode = "parallel";
// @ts-expect-error ToolExecutionMode excludes the empty literal.
toolExecutionMode = "";
// @ts-expect-error ToolExecutionMode excludes the unsupported mode.
toolExecutionMode = "concurrent";

let agentThinkingLevel: AgentThinkingLevel = "off";
agentThinkingLevel = "minimal";
agentThinkingLevel = "low";
agentThinkingLevel = "medium";
agentThinkingLevel = "high";
agentThinkingLevel = "xhigh";
agentThinkingLevel = "max";
agentThinkingLevel = "ultra";
// @ts-expect-error Agent package ThinkingLevel excludes the empty literal.
agentThinkingLevel = "";
// @ts-expect-error Agent package ThinkingLevel excludes unsupported literals.
agentThinkingLevel = "extreme";

let agentToolExecutionContext: AgentToolExecutionContext = {
	startedAt: 0,
};
agentToolExecutionContext.startedAt = -0;
agentToolExecutionContext.startedAt = Number.NaN;
agentToolExecutionContext.startedAt = Number.POSITIVE_INFINITY;
agentToolExecutionContext.startedAt = Number.NEGATIVE_INFINITY;
agentToolExecutionContext.agentId = "agent-id";
agentToolExecutionContext.agentId = "";
agentToolExecutionContext.agentId = undefined;
const agentToolExecutionContextWithAgentId: AgentToolExecutionContext = {
	startedAt: 1.5,
	agentId: "present-agent",
};
const agentToolExecutionContextWithOnlyRequiredField: AgentToolExecutionContext = {
	startedAt: 0,
};
// @ts-expect-error AgentToolExecutionContext requires startedAt.
const agentToolExecutionContextMissingStartedAt: AgentToolExecutionContext = {
	agentId: "missing-started-at",
};
const agentToolExecutionContextWrongAgentId: AgentToolExecutionContext = {
	startedAt: 0,
	// @ts-expect-error AgentToolExecutionContext agentId must be a string.
	agentId: 42,
};
void agentToolExecutionContextWithAgentId;
void agentToolExecutionContextWithOnlyRequiredField;
void agentToolExecutionContextMissingStartedAt;
void agentToolExecutionContextWrongAgentId;

let promptDiagnostic: PromptTemplateDiagnostic = {
	type: "warning",
	code: "file_info_failed",
	message: "diagnostic message",
	path: "/prompt.md",
};
promptDiagnostic.type = "warning";
promptDiagnostic.code = "parse_failed";
promptDiagnostic.message = "updated diagnostic message";
promptDiagnostic.path = "updated/path.md";
const diagnosticCode: PromptTemplateDiagnosticCode = "list_failed";
void diagnosticCode;

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
void compactionSettings;
void compactionDecision;
void toolExecutionMode;
void agentThinkingLevel;
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
