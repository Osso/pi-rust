import { createRequire } from "node:module";
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
const publicIndexImportPath = publicIndexPath.endsWith(".ts") ? publicIndexPath.slice(0, -3) : publicIndexPath;
const definingTypesPath = join(sourceWorktree, "packages/agent/src/types.ts");
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const typescriptPath = join(sourceWorktree, "node_modules/typescript/lib/typescript.js");
const typeRoots = join(sourceWorktree, "node_modules/@types");
const require = createRequire(import.meta.url);
const ts = require(typescriptPath);

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
	BeforeToolCallResult,
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

type ExpectedBeforeToolCallResult = {
	block?: boolean;
	reason?: string;
	terminate?: boolean;
};
type BeforeToolCallResultIsExact = Assert<
	Exact<BeforeToolCallResult, ExpectedBeforeToolCallResult>
>;
type BeforeToolCallResultBlockIsOptionalBoolean = Assert<
	Exact<BeforeToolCallResult["block"], boolean | undefined>
>;
type BeforeToolCallResultReasonIsOptionalString = Assert<
	Exact<BeforeToolCallResult["reason"], string | undefined>
>;
type BeforeToolCallResultTerminateIsOptionalBoolean = Assert<
	Exact<BeforeToolCallResult["terminate"], boolean | undefined>
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

let beforeToolCallResult: BeforeToolCallResult = {};
beforeToolCallResult.block = false;
beforeToolCallResult.block = true;
beforeToolCallResult.block = undefined;
beforeToolCallResult.reason = "";
beforeToolCallResult.reason = "blocked\\n理由\\0";
beforeToolCallResult.reason = undefined;
beforeToolCallResult.terminate = false;
beforeToolCallResult.terminate = true;
beforeToolCallResult.terminate = undefined;
const beforeToolCallResultPartial: BeforeToolCallResult = { block: true };
const beforeToolCallResultFull: BeforeToolCallResult = {
	block: false,
	reason: "blocked",
	terminate: true,
};
const beforeToolCallResultWithExplicitUndefined: BeforeToolCallResult = {
	block: undefined,
	reason: undefined,
	terminate: undefined,
};
void beforeToolCallResult;
void beforeToolCallResultPartial;
void beforeToolCallResultFull;
void beforeToolCallResultWithExplicitUndefined;

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

const excessPropertyFixtureSource = `
import type { BeforeToolCallResult } from ${JSON.stringify(publicIndexImportPath)};
const invalid: BeforeToolCallResult = { extra: true };
void invalid;
`;

function formatDiagnostics(diagnostics) {
	return ts.formatDiagnosticsWithColorAndContext(diagnostics, {
		getCanonicalFileName: (fileName) => fileName,
		getCurrentDirectory: () => sourceWorktree,
		getNewLine: () => "\\n",
	});
}

function readPinnedCompilerOptions() {
	const configFile = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
	if (configFile.error !== undefined) {
		throw new Error(`pinned tsconfig read failed: ${formatDiagnostics([configFile.error])}`);
	}
	const parsed = ts.parseJsonConfigFileContent(
		configFile.config,
		ts.sys,
		sourceWorktree,
		undefined,
		tsconfigPath,
	);
	if (parsed.errors.length > 0) {
		throw new Error(`pinned tsconfig parse failed: ${formatDiagnostics(parsed.errors)}`);
	}
	if (parsed.options.strict !== true || parsed.options.skipLibCheck !== true) {
		throw new Error("pinned tsconfig must retain strict and skipLibCheck=true");
	}
	if (parsed.options.exactOptionalPropertyTypes === true) {
		throw new Error("pinned tsconfig unexpectedly enables exactOptionalPropertyTypes");
	}
	return parsed.options;
}

function sourceFileAt(program, path, label) {
	const sourceFile = program.getSourceFile(path);
	if (sourceFile === undefined) throw new Error(`could not load real ${label}: ${path}`);
	return sourceFile;
}

function assertNoSourceDiagnostics(program, sourceFile, label) {
	const diagnostics = [
		...program.getSyntacticDiagnostics(sourceFile),
		...program.getSemanticDiagnostics(sourceFile),
	];
	if (diagnostics.length > 0) {
		throw new Error(`${label} diagnostics failed:\\n${formatDiagnostics(diagnostics)}`);
	}
}

function moduleExports(checker, sourceFile, label) {
	const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
	if (moduleSymbol === undefined) throw new Error(`could not obtain ${label} module symbol`);
	return checker.getExportsOfModule(moduleSymbol);
}

function assertWritable(property, sourceFile, label) {
	const declarations = property.declarations ?? [];
	if (declarations.length === 0) throw new Error(`${label} has no declarations`);
	for (const declaration of declarations) {
		const modifiers = ts.getCombinedModifierFlags(declaration);
		if ((modifiers & ts.ModifierFlags.Readonly) !== 0) {
			throw new Error(`${label} is readonly`);
		}
		if (declaration.getSourceFile() !== sourceFile) {
			throw new Error(`${label} declaration escaped defining types.ts`);
		}
	}
}

function assertOptionalPropertyType(checker, property, sourceFile, expectedFlag, label) {
	if ((property.flags & ts.SymbolFlags.Optional) === 0) {
		throw new Error(`${label} is not optional`);
	}
	assertWritable(property, sourceFile, label);
	const type = checker.getTypeOfSymbolAtLocation(property, sourceFile);
	const members = type.isUnion() ? type.types : [type];
	const hasUndefined = members.some((member) => (member.flags & ts.TypeFlags.Undefined) !== 0);
	const valueMembers = members.filter((member) => (member.flags & ts.TypeFlags.Undefined) === 0);
	const hasExpectedValueType = expectedFlag === ts.TypeFlags.Boolean
		? valueMembers.length === 2 && valueMembers.every((member) => (member.flags & ts.TypeFlags.BooleanLiteral) !== 0)
		: valueMembers.length === 1 && (valueMembers[0].flags & expectedFlag) !== 0;
	if (!hasUndefined || !hasExpectedValueType) {
		throw new Error(`${label} does not have the expected optional value type`);
	}
}

function resolveBeforeToolCallResultTarget(checker, indexFile, definingFile) {
	const indexExports = moduleExports(checker, indexFile, "real agent package index");
	const publicSymbol = indexExports.find((symbol) => symbol.name === "BeforeToolCallResult");
	if (publicSymbol === undefined) throw new Error("real agent package index does not export BeforeToolCallResult");

	const isAlias = (publicSymbol.flags & ts.SymbolFlags.Alias) !== 0;
	const publicDeclarations = publicSymbol.declarations ?? [];
	const target = isAlias ? checker.getAliasedSymbol(publicSymbol) : publicSymbol;
	const targetDeclarations = target.declarations ?? [];
	if (targetDeclarations.length !== 1 || targetDeclarations[0].getSourceFile() !== definingFile) {
		throw new Error("BeforeToolCallResult does not resolve to the real agent types module");
	}
	if (targetDeclarations[0].kind !== ts.SyntaxKind.InterfaceDeclaration) {
		throw new Error("BeforeToolCallResult target is not the real interface declaration");
	}
	if (isAlias && (publicDeclarations.length === 0 || publicDeclarations.some((declaration) => declaration.getSourceFile() !== indexFile))) {
		throw new Error("BeforeToolCallResult alias declaration escaped the real package index");
	}
	if (!isAlias && (publicDeclarations.length === 0 || publicDeclarations.some((declaration) => declaration.getSourceFile() !== definingFile))) {
		throw new Error("BeforeToolCallResult wildcard export does not resolve to the defining module");
	}
	return target;
}

function assertBeforeToolCallResultProperties(checker, target, definingFile) {
	const properties = checker.getDeclaredTypeOfSymbol(target).getProperties();
	const names = properties.map((property) => property.name);
	if (names.join("\\0") !== "block\\0reason\\0terminate") {
		throw new Error(`BeforeToolCallResult properties differ: ${names.join(",")}`);
	}
	assertOptionalPropertyType(checker, properties[0], definingFile, ts.TypeFlags.Boolean, "BeforeToolCallResult.block");
	assertOptionalPropertyType(checker, properties[1], definingFile, ts.TypeFlags.String, "BeforeToolCallResult.reason");
	assertOptionalPropertyType(checker, properties[2], definingFile, ts.TypeFlags.Boolean, "BeforeToolCallResult.terminate");
}

function proveBeforeToolCallResultExport(options) {
	const program = ts.createProgram({ rootNames: [publicIndexPath, definingTypesPath], options });
	const indexFile = sourceFileAt(program, publicIndexPath, "real agent package index");
	const definingFile = sourceFileAt(program, definingTypesPath, "real agent types module");
	assertNoSourceDiagnostics(program, indexFile, "real agent package index");
	assertNoSourceDiagnostics(program, definingFile, "real agent types module");

	const target = resolveBeforeToolCallResultTarget(program.getTypeChecker(), indexFile, definingFile);
	assertBeforeToolCallResultProperties(program.getTypeChecker(), target, definingFile);
	return { compilerVersion: ts.version, exactOptionalPropertyTypes: options.exactOptionalPropertyTypes === true };
}

function proveFreshExcessPropertyDiagnostic(options) {
	const temporaryDirectory = mkdtempSync(join(tmpdir(), "pi-before-tool-call-result-excess-"));
	const fixturePath = join(temporaryDirectory, "excess-property.ts");
	writeFileSync(fixturePath, excessPropertyFixtureSource, "utf8");
	try {
		const diagnosticOptions = {
			...options,
			noEmit: true,
		};
		const program = ts.createProgram({ rootNames: [fixturePath], options: diagnosticOptions });
		const sourceFile = sourceFileAt(program, fixturePath, "excess-property fixture");
		const syntacticDiagnostics = program.getSyntacticDiagnostics(sourceFile);
		if (syntacticDiagnostics.length !== 0) {
			throw new Error(`unexpected excess-property syntax diagnostics: ${formatDiagnostics(syntacticDiagnostics)}`);
		}
		const diagnostics = program.getSemanticDiagnostics(sourceFile);
		if (diagnostics.length !== 1) {
			throw new Error(`unexpected excess-property semantic diagnostic count: ${diagnostics.length}`);
		}
		const [diagnostic] = diagnostics;
		if (diagnostic.code !== 2353) {
			throw new Error(`unexpected excess-property diagnostic code: ${diagnostic.code}`);
		}
		if (diagnostic.category !== ts.DiagnosticCategory.Error) {
			throw new Error(`excess-property diagnostic is not an error: category=${diagnostic.category}`);
		}
		const expectedText = "extra";
		const expectedStart = excessPropertyFixtureSource.indexOf(expectedText);
		const actualText = diagnostic.start === undefined
			? ""
			: sourceFile.text.slice(diagnostic.start, diagnostic.start + (diagnostic.length ?? 0));
		if (
			diagnostic.file !== sourceFile
			|| expectedStart < 0
			|| diagnostic.start !== expectedStart
			|| diagnostic.length !== expectedText.length
			|| actualText !== expectedText
		) {
			throw new Error(
				`excess-property diagnostic span does not identify generated extra property: `
					+ `start=${diagnostic.start}, length=${diagnostic.length}, text=${JSON.stringify(actualText)}`,
			);
		}
	} finally {
		rmSync(temporaryDirectory, { recursive: true, force: true });
	}
}

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
	const options = readPinnedCompilerOptions();
	const exportProof = proveBeforeToolCallResultExport(options);
	const typecheckStatus = typecheckFixture();
	if (typecheckStatus !== 0) {
		process.exitCode = typecheckStatus;
	} else {
		proveFreshExcessPropertyDiagnostic(options);
		console.log(`compiler=${exportProof.compilerVersion}`);
		console.log("BeforeToolCallResult export: real-index wildcard -> packages/agent/src/types.ts");
		console.log("BeforeToolCallResult properties: block, reason, terminate; optional, writable");
		console.log(`exactOptionalPropertyTypes=${exportProof.exactOptionalPropertyTypes}`);
		console.log("BeforeToolCallResult runtime interface identity: not claimed; source-shaped records only");
		process.exitCode = 0;
	}
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`public index typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
