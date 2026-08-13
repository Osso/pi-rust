import { readFile } from "node:fs/promises";
import { StringDecoder } from "node:string_decoder";
import { createContext, runInContext } from "node:vm";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type TruncationOptions = {
	maxLines?: number;
	maxBytes?: number;
};

type TruncationCase = {
	content: string;
	options: TruncationOptions;
};

type TruncateApi = {
	formatSize(bytes: number): string;
	truncateHead(content: string, options?: TruncationOptions): unknown;
	truncateTail(content: string, options?: TruncationOptions): unknown;
	truncateLine(line: string, maxChars?: number): { text: string; wasTruncated: boolean };
};

type ShellOutputApi = {
	sanitizeBinaryOutput(value: string): string;
};

type PromptTemplateApi = {
	parseCommandArgs(argsString: string): string[];
	substituteArgs(content: string, args: string[]): string;
	formatPromptTemplateInvocation(
		template: { name: string; description?: string; content: string },
		args?: string[],
	): string;
};

type SkillFormattingApi = {
	formatSkillInvocation(skill: MutableSkillRecord, additionalInstructions?: string): string;
};

type SystemPromptApi = {
	formatSkillsForSystemPrompt(skills: MutableSkillRecord[]): string;
};

type ImageApi = {
	detectSupportedImageMimeType(buffer: Uint8Array): string | undefined;
	encodeBase64(bytes: Uint8Array): string;
};

type EditDiffApi = {
	detectLineEnding(content: string): "\r\n" | "\n";
	normalizeToLF(text: string): string;
	restoreLineEndings(text: string, ending: "\r\n" | "\n"): string;
	stripBom(content: string): { bom: string; text: string };
};

type FileOperationsApi = {
	formatFileOperations(readFiles: string[], modifiedFiles: string[]): string;
};

type CompactionApi = {
	SUMMARIZATION_SYSTEM_PROMPT: string;
};

type FileKind = "file" | "directory" | "symlink";

type FileInfoRequest = {
	id: RequestId;
	operation: "file_info";
	name: string;
	path: string;
	kind: FileKind;
	sizeBits: string;
	mtimeMsBits: string;
};

type FileInfoRecord = {
	name: string;
	path: string;
	kind: FileKind;
	size: number;
	mtimeMs: number;
};

type NormalizedFileInfo = {
	name: string;
	path: string;
	kind: FileKind;
	sizeBits: string;
	mtimeMsBits: string;
};

type ResourceBooleanState = "absent" | "undefined" | "false" | "true";
type ResourceDescriptionState = "absent" | "undefined" | "empty" | "value";

type SkillInput = {
	name: string;
	description: string;
	content: string;
	filePath: string;
	disableModelInvocationState: ResourceBooleanState;
};

type PromptTemplateInput = {
	name: string;
	descriptionState: ResourceDescriptionState;
	descriptionValue: string;
	content: string;
};

type PromptTemplateDiagnosticCode = "file_info_failed" | "list_failed" | "read_failed" | "parse_failed";

type PromptTemplateDiagnosticCaseRequest = {
	name: string;
	type: "warning";
	code: PromptTemplateDiagnosticCode;
	message: string;
	path: string;
};

type PromptTemplateDiagnosticRequest = {
	id: RequestId;
	operation: "prompt_template_diagnostic";
	cases: PromptTemplateDiagnosticCaseRequest[];
};

type SkillArrayInput =
	| { state: "absent" }
	| { state: "empty" }
	| { state: "values"; values: SkillInput[] };

type PromptTemplateArrayInput =
	| { state: "absent" }
	| { state: "empty" }
	| { state: "values"; values: PromptTemplateInput[] };

type ResourceValuesRequest = {
	id: RequestId;
	operation: "resource_values";
	skills: SkillArrayInput;
	promptTemplates: PromptTemplateArrayInput;
};

type PromptTemplateRequest = {
	id: RequestId;
	operation: "prompt_template_helpers";
	argsString: string;
	args: string[];
	content: string;
	templateName: string;
	templateDescription?: string;
	templateContent?: string;
	omitFormatArgs: boolean;
};

type SkillInvocationRequest = {
	skill: SkillInput;
	additionalInstructionsState: "absent" | "empty" | "value";
	additionalInstructions: string;
};

type SkillFormattingRequest = {
	id: RequestId;
	operation: "skill_formatting";
	systemPromptCases: SkillInput[][];
	invocationCases: SkillInvocationRequest[];
};

type ImageCaseRequest = {
	name: string;
	bytesHex: string;
};

type ImageHelpersRequest = {
	id: RequestId;
	operation: "image_helpers";
	cases: ImageCaseRequest[];
};

type EditTextCaseRequest = {
	name: string;
	detectionInput: string;
	normalizationInput: string;
	restorationInput: string;
	restorationEnding: "\r\n" | "\n";
	bomInput: string;
};

type EditTextHelpersRequest = {
	id: RequestId;
	operation: "edit_text_helpers";
	cases: EditTextCaseRequest[];
};

type FileOperationsFormatRequest = {
	id: RequestId;
	operation: "file_operations_format";
	readFiles: string[];
	modifiedFiles: string[];
};

type SummaryMarkersRequest = {
	id: RequestId;
	operation: "summary_markers";
};

type BashExecutionCaseRequest = {
	name: string;
	command: string;
	output: string;
	exitCodeState: "undefined" | "value";
	exitCodeBits: string;
	cancelled: boolean;
	truncated: boolean;
	fullOutputPathState: "absent" | "undefined" | "value";
	fullOutputPath: string;
	timestampBits: string;
	excludeFromContextState: "absent" | "undefined" | "false" | "true";
};

type BashExecutionMessageRequest = {
	id: RequestId;
	operation: "bash_execution_message";
	cases: BashExecutionCaseRequest[];
};

type BranchSummaryCaseRequest = {
	name: string;
	summary: string;
	fromId: string;
	timestampBits: string;
};

type BranchSummaryMessageRequest = {
	id: RequestId;
	operation: "branch_summary_message";
	cases: BranchSummaryCaseRequest[];
};

type SummarizationSystemPromptRequest = {
	id: RequestId;
	operation: "summarization_system_prompt";
};

type CutPointResultCaseRequest = {
	name: string;
	firstKeptEntryIndexBits: string;
	turnStartIndexBits: string;
	isSplitTurn: boolean;
};

type CutPointResultRequest = {
	id: RequestId;
	operation: "cut_point_result";
	cases: CutPointResultCaseRequest[];
};

type ContextUsageEstimateCaseRequest = {
	name: string;
	tokensBits: string;
	usageTokensBits: string;
	trailingTokensBits: string;
	lastUsageIndexBits: string | null;
};

type ContextUsageEstimateRequest = {
	id: RequestId;
	operation: "context_usage_estimate";
	cases: ContextUsageEstimateCaseRequest[];
};

type CompactionDetailsCaseRequest = {
	name: string;
	readFiles: string[];
	modifiedFiles: string[];
};

type CompactionDetailsRequest = {
	id: RequestId;
	operation: "compaction_details";
	cases: CompactionDetailsCaseRequest[];
};

type QueueModeCaseRequest = {
	name: string;
	mode: "all" | "one-at-a-time";
};

type QueueModeRequest = {
	id: RequestId;
	operation: "queue_mode";
	cases: QueueModeCaseRequest[];
};

type ToolExecutionModeCaseRequest = {
	name: string;
	mode: "sequential" | "parallel";
};

type ToolExecutionModeRequest = {
	id: RequestId;
	operation: "tool_execution_mode";
	cases: ToolExecutionModeCaseRequest[];
};

type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra";

type ThinkingLevelCaseRequest = {
	name: string;
	level: ThinkingLevel;
};

type ThinkingLevelRequest = {
	id: RequestId;
	operation: "thinking_level";
	cases: ThinkingLevelCaseRequest[];
};

type AgentToolExecutionContextCaseRequest = {
	name: string;
	startedAtBits: string;
	agentIdState: "absent" | "present";
	agentId: string;
};

type AgentToolExecutionContextRequest = {
	id: RequestId;
	operation: "agent_tool_execution_context";
	cases: AgentToolExecutionContextCaseRequest[];
};

type MutableBashExecutionMessageRecord = {
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

type MutableBranchSummaryMessageRecord = {
	role: "branchSummary";
	summary: string;
	fromId: string;
	timestamp: number;
};

type MutableAgentToolExecutionContextRecord = {
	startedAt: number;
	agentId?: string;
};

type MutableCutPointResultRecord = {
	firstKeptEntryIndex: number;
	turnStartIndex: number;
	isSplitTurn: boolean;
};

type MutableContextUsageEstimateRecord = {
	tokens: number;
	usageTokens: number;
	trailingTokens: number;
	lastUsageIndex: number | null;
};

type MutablePromptTemplateDiagnosticRecord = {
	type: "warning";
	code: PromptTemplateDiagnosticCode;
	message: string;
	path: string;
};

type ShouldCompactCaseRequest = {
	name: string;
	contextTokensBits: string;
	contextWindowBits: string;
	enabled: boolean;
	reserveTokensBits: string;
	keepRecentTokensBits: string;
};

type ShouldCompactRequest = {
	id: RequestId;
	operation: "should_compact";
	cases: ShouldCompactCaseRequest[];
};

type MutableSkillRecord = {
	name: string;
	description: string;
	content: string;
	filePath: string;
	disableModelInvocation?: boolean;
};

type MutablePromptTemplateRecord = {
	name: string;
	description?: string;
	content: string;
};

type MutableResourcesRecord = {
	promptTemplates?: MutablePromptTemplateRecord[];
	skills?: MutableSkillRecord[];
};

type FileErrorCode =
	| "aborted"
	| "not_found"
	| "permission_denied"
	| "not_directory"
	| "is_directory"
	| "invalid"
	| "not_supported"
	| "unknown";

type ExecutionErrorCode =
	| "aborted"
	| "timeout"
	| "shell_unavailable"
	| "spawn_error"
	| "callback_error"
	| "unknown";

type CompactionErrorCode = "aborted" | "summarization_failed";
type BranchSummaryErrorCode = "aborted" | "summarization_failed";

type HarnessErrorCauseRequest =
	| { kind: "error"; message: string }
	| {
			kind: "file";
			code: FileErrorCode;
			message: string;
			path?: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			kind: "execution";
			code: ExecutionErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			kind: "compaction";
			code: CompactionErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			kind: "branch_summary";
			code: BranchSummaryErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		};

type HarnessErrorRequest =
	| {
			id: RequestId;
			operation: "harness_error";
			kind: "file";
			code: FileErrorCode;
			message: string;
			path?: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			id: RequestId;
			operation: "harness_error";
			kind: "execution";
			code: ExecutionErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			id: RequestId;
			operation: "harness_error";
			kind: "compaction";
			code: CompactionErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		}
	| {
			id: RequestId;
			operation: "harness_error";
			kind: "branch_summary";
			code: BranchSummaryErrorCode;
			message: string;
			cause?: HarnessErrorCauseRequest;
		};

type HarnessErrorInstance = Error & {
	code: FileErrorCode | ExecutionErrorCode | CompactionErrorCode | BranchSummaryErrorCode;
	path?: string;
};

type HarnessTypesApi = {
	FileError: new (code: FileErrorCode, message: string, path?: string, cause?: Error) => HarnessErrorInstance;
	ExecutionError: new (code: ExecutionErrorCode, message: string, cause?: Error) => HarnessErrorInstance;
	CompactionError: new (code: CompactionErrorCode, message: string, cause?: Error) => HarnessErrorInstance;
	BranchSummaryError: new (code: BranchSummaryErrorCode, message: string, cause?: Error) => HarnessErrorInstance;
	COMPACTION_SUMMARY_PREFIX: string;
	COMPACTION_SUMMARY_SUFFIX: string;
	BRANCH_SUMMARY_PREFIX: string;
	BRANCH_SUMMARY_SUFFIX: string;
	shouldCompact: (
		contextTokens: number,
		contextWindow: number,
		settings: { enabled: boolean; reserveTokens: number; keepRecentTokens: number },
	) => boolean;
};

type NormalizedHarnessError = {
	kind: "FileError" | "ExecutionError" | "CompactionError" | "BranchSummaryError" | "Error";
	name: string;
	message: string;
	code: FileErrorCode | ExecutionErrorCode | CompactionErrorCode | BranchSummaryErrorCode | null;
	path: string | null;
	cause: NormalizedHarnessError | null;
};

type Request =
	| {
			id: RequestId;
			operation: "format_size";
			bytes: number;
		}
	| {
			id: RequestId;
			operation: "truncate_head";
			content: string;
			options?: TruncationOptions;
		}
	| {
			id: RequestId;
			operation: "truncate_tail";
			content: string;
			options?: TruncationOptions;
		}
	| {
			id: RequestId;
			operation: "truncate_head_without_buffer";
			content: string;
			options?: TruncationOptions;
		}
	| {
			id: RequestId;
			operation: "truncate_line";
			line: string;
			maxChars?: number;
		}
	| {
			id: RequestId;
			operation: "truncate_tail_batch";
			cases: TruncationCase[];
		}
	| {
			id: RequestId;
			operation: "sanitize_binary_output";
			text: string;
		}
	| {
			id: RequestId;
			operation: "sanitize_utf16_units";
			units: number[];
		}
	| FileInfoRequest
	| ResourceValuesRequest
	| PromptTemplateRequest
	| SkillFormattingRequest
	| ImageHelpersRequest
	| EditTextHelpersRequest
	| FileOperationsFormatRequest
	| SummaryMarkersRequest
	| BashExecutionMessageRequest
	| BranchSummaryMessageRequest
	| ShouldCompactRequest
	| PromptTemplateDiagnosticRequest
	| SummarizationSystemPromptRequest
	| CutPointResultRequest
	| ContextUsageEstimateRequest
	| CompactionDetailsRequest
	| QueueModeRequest
	| ToolExecutionModeRequest
	| ThinkingLevelRequest
	| AgentToolExecutionContextRequest
	| HarnessErrorRequest;

type UtilityRequest = Exclude<
	Request,
	HarnessErrorRequest |
		SummaryMarkersRequest |
		ShouldCompactRequest |
		PromptTemplateDiagnosticRequest |
		SummarizationSystemPromptRequest |
		CutPointResultRequest |
		ContextUsageEstimateRequest |
		CompactionDetailsRequest |
		QueueModeRequest |
		ToolExecutionModeRequest |
		ThinkingLevelRequest |
		AgentToolExecutionContextRequest
>;
type FormatSizeRequest = Extract<UtilityRequest, { operation: "format_size" }>;
type TruncationRequest = Extract<
	UtilityRequest,
	{ operation: "truncate_head" | "truncate_tail" | "truncate_head_without_buffer" | "truncate_line" | "truncate_tail_batch" }
>;
type SanitizationRequest = Extract<UtilityRequest, { operation: "sanitize_binary_output" | "sanitize_utf16_units" }>;

type Response =
	| {
			id: RequestId;
			ok: true;
			output: unknown;
		}
	| {
			id: RequestId;
			ok: false;
			error: { name: string; message: string };
		};

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function toUtf16Units(value: string): number[] {
	return Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
}

function toCodePoints(value: string): number[] {
	return Array.from(value, (character) => character.codePointAt(0)).filter(
		(codePoint): codePoint is number => codePoint !== undefined,
	);
}

function normalizeSanitizedText(value: string): { text: string; utf16Units: number[]; codePoints: number[] } {
	return { text: value, utf16Units: toUtf16Units(value), codePoints: toCodePoints(value) };
}

function normalizeSanitizedUnits(value: string): { utf16Units: number[]; codePoints: number[] } {
	return { utf16Units: toUtf16Units(value), codePoints: toCodePoints(value) };
}

function binary64FromBits(bits: string): number {
	const buffer = new ArrayBuffer(8);
	const view = new DataView(buffer);
	view.setBigUint64(0, BigInt(`0x${bits}`), false);
	return view.getFloat64(0, false);
}

function binary64Bits(value: number): string {
	const buffer = new ArrayBuffer(8);
	const view = new DataView(buffer);
	view.setFloat64(0, value, false);
	return view.getBigUint64(0, false).toString(16).padStart(16, "0");
}

function normalizeFileInfo(request: FileInfoRequest): NormalizedFileInfo {
	const info: FileInfoRecord = {
		name: request.name,
		path: request.path,
		kind: request.kind,
		size: binary64FromBits(request.sizeBits),
		mtimeMs: binary64FromBits(request.mtimeMsBits),
	};
	return {
		name: info.name,
		path: info.path,
		kind: info.kind,
		sizeBits: binary64Bits(info.size),
		mtimeMsBits: binary64Bits(info.mtimeMs),
	};
}

function buildSkillRecord(input: SkillInput): MutableSkillRecord {
	const skill: MutableSkillRecord = {
		name: input.name,
		description: input.description,
		content: input.content,
		filePath: input.filePath,
	};
	switch (input.disableModelInvocationState) {
		case "undefined":
			skill.disableModelInvocation = undefined;
			break;
		case "false":
			skill.disableModelInvocation = false;
			break;
		case "true":
			skill.disableModelInvocation = true;
			break;
	}
	return skill;
}

function buildPromptTemplateRecord(input: PromptTemplateInput): MutablePromptTemplateRecord {
	const template: MutablePromptTemplateRecord = { name: input.name, content: input.content };
	switch (input.descriptionState) {
		case "undefined":
			template.description = undefined;
			break;
		case "empty":
			template.description = "";
			break;
		case "value":
			template.description = input.descriptionValue;
			break;
	}
	return template;
}

function buildSkillArray(input: SkillArrayInput): MutableSkillRecord[] | undefined {
	if (input.state === "absent") return undefined;
	if (input.state === "empty") return [];
	return input.values.map(buildSkillRecord);
}

function buildPromptTemplateArray(input: PromptTemplateArrayInput): MutablePromptTemplateRecord[] | undefined {
	if (input.state === "absent") return undefined;
	if (input.state === "empty") return [];
	return input.values.map(buildPromptTemplateRecord);
}

function buildResourceValues(request: ResourceValuesRequest): MutableResourcesRecord {
	const resources: MutableResourcesRecord = {};
	const skills = buildSkillArray(request.skills);
	const promptTemplates = buildPromptTemplateArray(request.promptTemplates);
	if (skills !== undefined) resources.skills = skills;
	if (promptTemplates !== undefined) resources.promptTemplates = promptTemplates;
	return resources;
}

function normalizeResourceValues(resources: MutableResourcesRecord): unknown {
	return {
		prompt_templates: normalizePromptTemplates(resources.promptTemplates),
		skills: normalizeSkills(resources.skills),
	};
}

function normalizeSkills(skills: MutableSkillRecord[] | undefined): unknown {
	return skills === undefined ? null : skills.map(normalizeSkill);
}

function normalizeSkill(skill: MutableSkillRecord): unknown {
	return {
		name: skill.name,
		description: skill.description,
		content: skill.content,
		file_path: skill.filePath,
		disable_model_invocation: skill.disableModelInvocation ?? null,
	};
}

function normalizePromptTemplates(templates: MutablePromptTemplateRecord[] | undefined): unknown {
	return templates === undefined ? null : templates.map(normalizePromptTemplate);
}

function normalizePromptTemplate(template: MutablePromptTemplateRecord): unknown {
	return {
		name: template.name,
		description: template.description ?? null,
		content: template.content,
	};
}

function executeResourceValues(request: ResourceValuesRequest): Response {
	return successfulResponse(request.id, normalizeResourceValues(buildResourceValues(request)));
}

function executePromptTemplateRequest(request: PromptTemplateRequest, api: PromptTemplateApi): Response {
	const template: { name: string; description?: string; content: string } = {
		name: request.templateName,
		content: request.templateContent ?? request.content,
	};
	if (request.templateDescription !== undefined) template.description = request.templateDescription;
	const formatted = request.omitFormatArgs
		? api.formatPromptTemplateInvocation(template)
		: api.formatPromptTemplateInvocation(template, request.args);
	return successfulResponse(request.id, {
		parsedArgs: api.parseCommandArgs(request.argsString),
		substituted: api.substituteArgs(request.content, request.args),
		formatted,
	});
}

function executeSkillFormattingRequest(
	request: SkillFormattingRequest,
	skillFormatting: SkillFormattingApi,
	systemPrompt: SystemPromptApi,
): Response {
	const systemPrompts = request.systemPromptCases.map((skills) =>
		systemPrompt.formatSkillsForSystemPrompt(skills.map(buildSkillRecord)),
	);
	const invocations = request.invocationCases.map((invocation) => {
		const skill = buildSkillRecord(invocation.skill);
		switch (invocation.additionalInstructionsState) {
			case "absent":
				return skillFormatting.formatSkillInvocation(skill);
			case "empty":
				return skillFormatting.formatSkillInvocation(skill, "");
			case "value":
				return skillFormatting.formatSkillInvocation(skill, invocation.additionalInstructions);
		}
	});
	return successfulResponse(request.id, { systemPrompts, invocations });
}

function executeImageHelpers(request: ImageHelpersRequest, api: ImageApi): Response {
	const results = request.cases.map((imageCase) => {
		const bytes = Buffer.from(imageCase.bytesHex, "hex");
		return {
			name: imageCase.name,
			mimeType: api.detectSupportedImageMimeType(bytes) ?? null,
			base64: api.encodeBase64(bytes),
		};
	});
	return successfulResponse(request.id, { results });
}

function executeEditTextHelpers(request: EditTextHelpersRequest, api: EditDiffApi): Response {
	const results = request.cases.map((editCase) => {
		const stripped = api.stripBom(editCase.bomInput);
		return {
			name: editCase.name,
			detectedEnding: api.detectLineEnding(editCase.detectionInput),
			normalized: api.normalizeToLF(editCase.normalizationInput),
			restored: api.restoreLineEndings(editCase.restorationInput, editCase.restorationEnding),
			stripped,
		};
	});
	return successfulResponse(request.id, { results });
}

function executeFileOperationsRequest(request: FileOperationsFormatRequest, api: FileOperationsApi): Response {
	return successfulResponse(request.id, api.formatFileOperations(request.readFiles, request.modifiedFiles));
}

function successfulResponse(id: RequestId, output: unknown): Response {
	return { id, ok: true, output };
}

function normalizeTruncatedLine(
	api: TruncateApi,
	line: string,
	maxChars?: number,
): { textUtf16: number[]; wasTruncated: boolean } {
	const result = api.truncateLine(line, maxChars);
	return { textUtf16: toUtf16Units(result.text), wasTruncated: result.wasTruncated };
}

function normalizeTruncatedTailBatch(api: TruncateApi, cases: TruncationCase[]): { results: unknown[] } {
	return { results: cases.map(({ content, options }) => api.truncateTail(content, options)) };
}

function normalizeSanitizedUtf16Units(
	api: ShellOutputApi,
	units: number[],
): { utf16Units: number[]; codePoints: number[] } {
	const value = String.fromCharCode(...units);
	return normalizeSanitizedUnits(api.sanitizeBinaryOutput(value));
}

function normalizeHarnessError(error: Error, api: HarnessTypesApi): NormalizedHarnessError {
	const harnessError = error as HarnessErrorInstance;
	const cause = normalizeHarnessCause(error, api);
	if (error instanceof api.FileError) {
		return normalizeFileHarnessError(error, harnessError, cause);
	}
	if (error instanceof api.ExecutionError) {
		return normalizeExecutionHarnessError(error, harnessError, cause);
	}
	if (error instanceof api.CompactionError) {
		return normalizeCompactionHarnessError(error, harnessError, cause);
	}
	if (error instanceof api.BranchSummaryError) {
		return normalizeBranchSummaryHarnessError(error, harnessError, cause);
	}
	return normalizeUnknownHarnessError(error, cause);
}

function normalizeHarnessCause(error: Error, api: HarnessTypesApi): NormalizedHarnessError | null {
	return error.cause instanceof Error ? normalizeHarnessError(error.cause, api) : null;
}

function normalizeFileHarnessError(
	error: Error,
	harnessError: HarnessErrorInstance,
	cause: NormalizedHarnessError | null,
): NormalizedHarnessError {
	return {
		kind: "FileError",
		name: error.name,
		message: error.message,
		code: harnessError.code,
		path: harnessError.path ?? null,
		cause,
	};
}

function normalizeExecutionHarnessError(
	error: Error,
	harnessError: HarnessErrorInstance,
	cause: NormalizedHarnessError | null,
): NormalizedHarnessError {
	return {
		kind: "ExecutionError",
		name: error.name,
		message: error.message,
		code: harnessError.code,
		path: null,
		cause,
	};
}

function normalizeCompactionHarnessError(
	error: Error,
	harnessError: HarnessErrorInstance,
	cause: NormalizedHarnessError | null,
): NormalizedHarnessError {
	return {
		kind: "CompactionError",
		name: error.name,
		message: error.message,
		code: harnessError.code,
		path: null,
		cause,
	};
}

function normalizeBranchSummaryHarnessError(
	error: Error,
	harnessError: HarnessErrorInstance,
	cause: NormalizedHarnessError | null,
): NormalizedHarnessError {
	return {
		kind: "BranchSummaryError",
		name: error.name,
		message: error.message,
		code: harnessError.code,
		path: null,
		cause,
	};
}

function normalizeUnknownHarnessError(error: Error, cause: NormalizedHarnessError | null): NormalizedHarnessError {
	return {
		kind: "Error",
		name: error.name,
		message: error.message,
		code: null,
		path: null,
		cause,
	};
}

function buildHarnessCause(request: HarnessErrorCauseRequest, api: HarnessTypesApi): Error {
	if (request.kind === "error") return new Error(request.message);
	const cause = request.cause === undefined ? undefined : buildHarnessCause(request.cause, api);
	if (request.kind === "file") return new api.FileError(request.code, request.message, request.path, cause);
	if (request.kind === "execution") return new api.ExecutionError(request.code, request.message, cause);
	if (request.kind === "compaction") return new api.CompactionError(request.code, request.message, cause);
	return new api.BranchSummaryError(request.code, request.message, cause);
}

function executeHarnessError(request: HarnessErrorRequest, api: HarnessTypesApi): Response {
	const cause = request.cause === undefined ? undefined : buildHarnessCause(request.cause, api);
	const error = buildHarnessError(request, api, cause);
	return successfulResponse(request.id, normalizeHarnessError(error, api));
}

function executeSummaryMarkers(request: SummaryMarkersRequest, api: HarnessTypesApi): Response {
	return successfulResponse(request.id, {
		compactionPrefix: api.COMPACTION_SUMMARY_PREFIX,
		compactionSuffix: api.COMPACTION_SUMMARY_SUFFIX,
		branchPrefix: api.BRANCH_SUMMARY_PREFIX,
		branchSuffix: api.BRANCH_SUMMARY_SUFFIX,
	});
}

function executeBashExecutionMessage(request: BashExecutionMessageRequest): Response {
	const results = request.cases.map(normalizeBashExecutionInput);
	return successfulResponse(request.id, { results });
}

function normalizeBashExecutionInput(input: BashExecutionCaseRequest): unknown {
	const message = buildBashExecutionMessage(input);
	return normalizeBashExecutionMessage(input.name, message);
}

function buildBashExecutionMessage(input: BashExecutionCaseRequest): MutableBashExecutionMessageRecord {
	const message: MutableBashExecutionMessageRecord = {
		role: "bashExecution",
		command: input.command,
		output: input.output,
		exitCode: input.exitCodeState === "undefined" ? undefined : binary64FromBits(input.exitCodeBits),
		cancelled: input.cancelled,
		truncated: input.truncated,
		timestamp: binary64FromBits(input.timestampBits),
	};
	assignFullOutputPath(message, input);
	assignExcludeFromContext(message, input);
	return message;
}

function assignFullOutputPath(message: MutableBashExecutionMessageRecord, input: BashExecutionCaseRequest): void {
	if (input.fullOutputPathState === "undefined") message.fullOutputPath = undefined;
	if (input.fullOutputPathState === "value") message.fullOutputPath = input.fullOutputPath;
}

function assignExcludeFromContext(message: MutableBashExecutionMessageRecord, input: BashExecutionCaseRequest): void {
	if (input.excludeFromContextState === "undefined") message.excludeFromContext = undefined;
	if (input.excludeFromContextState === "false") message.excludeFromContext = false;
	if (input.excludeFromContextState === "true") message.excludeFromContext = true;
}

function normalizeBashExecutionMessage(name: string, message: MutableBashExecutionMessageRecord): unknown {
	return {
		name,
		role: message.role,
		command: message.command,
		output: message.output,
		exitCodeBits: message.exitCode === undefined ? null : binary64Bits(message.exitCode),
		cancelled: message.cancelled,
		truncated: message.truncated,
		fullOutputPath: message.fullOutputPath ?? null,
		timestampBits: binary64Bits(message.timestamp),
		excludeFromContext: message.excludeFromContext ?? null,
	};
}

function executeBranchSummaryMessage(request: BranchSummaryMessageRequest): Response {
	const results = request.cases.map(normalizeBranchSummaryInput);
	return successfulResponse(request.id, { results });
}

function executePromptTemplateDiagnosticRequest(request: PromptTemplateDiagnosticRequest): Response {
	const results = request.cases.map(normalizePromptTemplateDiagnosticInput);
	return successfulResponse(request.id, { results });
}

function executeSummarizationSystemPromptRequest(request: SummarizationSystemPromptRequest, api: CompactionApi): Response {
	return successfulResponse(request.id, normalizeSummarizationSystemPrompt(api.SUMMARIZATION_SYSTEM_PROMPT));
}

function executeCutPointResultRequest(request: CutPointResultRequest): Response {
	const results = request.cases.map(normalizeCutPointResultInput);
	return successfulResponse(request.id, { results });
}

function normalizeSummarizationSystemPrompt(value: string): {
	value: string;
	utf8Bytes: number;
	utf16Units: number;
	lineFeedPositions: number[];
	hasCarriageReturn: boolean;
	endsWithLineFeed: boolean;
} {
	const lineFeedPositions = Array.from(value).flatMap((character, index) => (character === "\n" ? [index] : []));
	return {
		value,
		utf8Bytes: Buffer.byteLength(value, "utf8"),
		utf16Units: value.length,
		lineFeedPositions,
		hasCarriageReturn: value.includes("\r"),
		endsWithLineFeed: value.endsWith("\n"),
	};
}

function normalizePromptTemplateDiagnosticInput(input: PromptTemplateDiagnosticCaseRequest): unknown {
	const diagnostic: MutablePromptTemplateDiagnosticRecord = {
		type: input.type,
		code: input.code,
		message: input.message,
		path: input.path,
	};
	return {
		name: input.name,
		type: diagnostic.type,
		code: diagnostic.code,
		message: diagnostic.message,
		path: diagnostic.path,
	};
}

function normalizeCutPointResultInput(input: CutPointResultCaseRequest): unknown {
	const result: MutableCutPointResultRecord = {
		firstKeptEntryIndex: binary64FromBits(input.firstKeptEntryIndexBits),
		turnStartIndex: binary64FromBits(input.turnStartIndexBits),
		isSplitTurn: input.isSplitTurn,
	};
	return {
		name: input.name,
		firstKeptEntryIndexBits: binary64Bits(result.firstKeptEntryIndex),
		turnStartIndexBits: binary64Bits(result.turnStartIndex),
		isSplitTurn: result.isSplitTurn,
	};
}

function executeContextUsageEstimateRequest(request: ContextUsageEstimateRequest): Response {
	const results = request.cases.map(normalizeContextUsageEstimateInput);
	return successfulResponse(request.id, { results });
}

function normalizeContextUsageEstimateInput(input: ContextUsageEstimateCaseRequest): unknown {
	const estimate: MutableContextUsageEstimateRecord = {
		tokens: binary64FromBits(input.tokensBits),
		usageTokens: binary64FromBits(input.usageTokensBits),
		trailingTokens: binary64FromBits(input.trailingTokensBits),
		lastUsageIndex: input.lastUsageIndexBits === null ? null : binary64FromBits(input.lastUsageIndexBits),
	};
	return {
		name: input.name,
		tokensBits: binary64Bits(estimate.tokens),
		usageTokensBits: binary64Bits(estimate.usageTokens),
		trailingTokensBits: binary64Bits(estimate.trailingTokens),
		lastUsageIndexBits: estimate.lastUsageIndex === null ? null : binary64Bits(estimate.lastUsageIndex),
	};
}

function executeCompactionDetailsRequest(request: CompactionDetailsRequest): Response {
	const results = request.cases.map((input) => ({
		name: input.name,
		readFiles: [...input.readFiles],
		modifiedFiles: [...input.modifiedFiles],
	}));
	return successfulResponse(request.id, { results });
}

function executeQueueModeRequest(request: QueueModeRequest): Response {
	const results = request.cases.map((input) => ({
		name: input.name,
		mode: input.mode,
	}));
	return successfulResponse(request.id, { results });
}

function executeToolExecutionModeRequest(request: ToolExecutionModeRequest): Response {
	const results = request.cases.map((input) => ({
		name: input.name,
		mode: input.mode,
	}));
	return successfulResponse(request.id, { results });
}

function executeThinkingLevelRequest(request: ThinkingLevelRequest): Response {
	const results = request.cases.map((input) => ({
		name: input.name,
		level: input.level,
	}));
	return successfulResponse(request.id, { results });
}

function executeAgentToolExecutionContextRequest(request: AgentToolExecutionContextRequest): Response {
	const results = request.cases.map(normalizeAgentToolExecutionContextInput);
	return successfulResponse(request.id, { results });
}

function normalizeAgentToolExecutionContextInput(input: AgentToolExecutionContextCaseRequest): unknown {
	const context: MutableAgentToolExecutionContextRecord = {
		startedAt: binary64FromBits(input.startedAtBits),
	};
	if (input.agentIdState === "present") context.agentId = input.agentId;
	return {
		name: input.name,
		startedAtBits: binary64Bits(context.startedAt),
		agentId: context.agentId ?? null,
	};
}

function executeShouldCompactRequest(request: ShouldCompactRequest, api: HarnessTypesApi): Response {
	const results = request.cases.map((input) => ({
		name: input.name,
		shouldCompact: api.shouldCompact(
			binary64FromBits(input.contextTokensBits),
			binary64FromBits(input.contextWindowBits),
			{
				enabled: input.enabled,
				reserveTokens: binary64FromBits(input.reserveTokensBits),
				keepRecentTokens: binary64FromBits(input.keepRecentTokensBits),
			},
		),
	}));
	return successfulResponse(request.id, { results });
}

function normalizeBranchSummaryInput(input: BranchSummaryCaseRequest): unknown {
	const message: MutableBranchSummaryMessageRecord = {
		role: "branchSummary",
		summary: input.summary,
		fromId: input.fromId,
		timestamp: binary64FromBits(input.timestampBits),
	};
	return {
		name: input.name,
		role: message.role,
		summary: message.summary,
		fromId: message.fromId,
		timestampBits: binary64Bits(message.timestamp),
	};
}

function buildHarnessError(request: HarnessErrorRequest, api: HarnessTypesApi, cause: Error | undefined): Error {
	if (request.kind === "file") return new api.FileError(request.code, request.message, request.path, cause);
	if (request.kind === "execution") return new api.ExecutionError(request.code, request.message, cause);
	if (request.kind === "compaction") return new api.CompactionError(request.code, request.message, cause);
	return new api.BranchSummaryError(request.code, request.message, cause);
}

function executeFormatSizeRequest(request: FormatSizeRequest, api: TruncateApi): Response {
	return successfulResponse(request.id, { value: api.formatSize(request.bytes) });
}

function executeTruncationRequest(request: TruncationRequest, normal: TruncateApi, fallback: TruncateApi): Response {
	if (request.operation === "truncate_head") {
		return successfulResponse(request.id, normal.truncateHead(request.content, request.options));
	}
	if (request.operation === "truncate_tail") {
		return successfulResponse(request.id, normal.truncateTail(request.content, request.options));
	}
	if (request.operation === "truncate_head_without_buffer") {
		return successfulResponse(request.id, fallback.truncateHead(request.content, request.options));
	}
	if (request.operation === "truncate_line") {
		return successfulResponse(request.id, normalizeTruncatedLine(normal, request.line, request.maxChars));
	}
	return successfulResponse(request.id, normalizeTruncatedTailBatch(normal, request.cases));
}

function executeSanitizationRequest(request: SanitizationRequest, api: ShellOutputApi): Response {
	if (request.operation === "sanitize_binary_output") {
		return successfulResponse(request.id, normalizeSanitizedText(api.sanitizeBinaryOutput(request.text)));
	}
	return successfulResponse(request.id, normalizeSanitizedUtf16Units(api, request.units));
}

function executeUtilityRequest(
	request: UtilityRequest,
	normal: TruncateApi,
	fallback: TruncateApi,
	shellOutput: ShellOutputApi,
	promptTemplates: PromptTemplateApi,
	skillFormatting: SkillFormattingApi,
	systemPrompt: SystemPromptApi,
	image: ImageApi,
	editDiff: EditDiffApi,
	fileOperations: FileOperationsApi,
): Response {
	switch (request.operation) {
		case "format_size":
			return executeFormatSizeRequest(request, normal);
		case "truncate_head":
		case "truncate_tail":
		case "truncate_head_without_buffer":
		case "truncate_line":
		case "truncate_tail_batch":
			return executeTruncationRequest(request, normal, fallback);
		case "sanitize_binary_output":
		case "sanitize_utf16_units":
			return executeSanitizationRequest(request, shellOutput);
		case "file_info":
			return successfulResponse(request.id, normalizeFileInfo(request));
		case "resource_values":
			return executeResourceValues(request);
		case "prompt_template_helpers":
			return executePromptTemplateRequest(request, promptTemplates);
		case "skill_formatting":
			return executeSkillFormattingRequest(request, skillFormatting, systemPrompt);
		case "image_helpers":
			return executeImageHelpers(request, image);
		case "edit_text_helpers":
			return executeEditTextHelpers(request, editDiff);
		case "file_operations_format":
			return executeFileOperationsRequest(request, fileOperations);
		case "bash_execution_message":
			return executeBashExecutionMessage(request);
		case "branch_summary_message":
			return executeBranchSummaryMessage(request);
	}
}

function executeSpecialRequest(request: Request, apis: OracleApis): Response | undefined {
	if (request.operation === "summary_markers") return executeSummaryMarkers(request, apis.harnessTypes);
	if (request.operation === "should_compact") return executeShouldCompactRequest(request, apis.harnessTypes);
	if (request.operation === "prompt_template_diagnostic") return executePromptTemplateDiagnosticRequest(request);
	if (request.operation === "summarization_system_prompt") {
		return executeSummarizationSystemPromptRequest(request, apis.compaction);
	}
	if (request.operation === "cut_point_result") return executeCutPointResultRequest(request);
	if (request.operation === "context_usage_estimate") return executeContextUsageEstimateRequest(request);
	if (request.operation === "compaction_details") return executeCompactionDetailsRequest(request);
	if (request.operation === "queue_mode") return executeQueueModeRequest(request);
	if (request.operation === "tool_execution_mode") return executeToolExecutionModeRequest(request);
	if (request.operation === "thinking_level") return executeThinkingLevelRequest(request);
	if (request.operation === "agent_tool_execution_context") {
		return executeAgentToolExecutionContextRequest(request);
	}
	if (request.operation === "harness_error") return executeHarnessError(request, apis.harnessTypes);
	return undefined;
}

function dispatchRequest(request: Request, apis: OracleApis): Response {
	const specialResponse = executeSpecialRequest(request, apis);
	if (specialResponse !== undefined) return specialResponse;
	return executeUtilityRequest(
		request as UtilityRequest,
		apis.normal,
		apis.fallback,
		apis.shellOutput,
		apis.promptTemplates,
		apis.skillFormatting,
		apis.systemPrompt,
		apis.image,
		apis.editDiff,
		apis.fileOperations,
	);
}

function execute(request: Request, apis: OracleApis): Response {
	try {
		return dispatchRequest(request, apis);
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

type EsbuildApi = {
	transform(
		source: string,
		options: { loader: "ts"; format: "cjs"; target: "es2022" },
	): Promise<{ code: string }>;
};

type EsbuildModule = {
	transform?: EsbuildApi["transform"];
	default?: EsbuildApi;
};

type CommonJsModule = {
	exports: Record<string, unknown>;
};

type ModulePaths = {
	truncate: string;
	shellOutput: string;
	agent: string;
	promptTemplates: string;
	skills: string;
	systemPrompt: string;
	image: string;
	editDiff: string;
	fileOperations: string;
	compaction: string;
	esbuild: string;
};

const MODULE_PATH_ERROR =
	"agent utility oracle requires truncate, shell-output, agent-package, prompt-template, skills, system-prompt, image, edit-diff, file-operations, compaction, and esbuild module paths";

function requireModulePath(index: number): string {
	const modulePath = process.argv[index];
	if (modulePath === undefined) throw new Error(MODULE_PATH_ERROR);
	return modulePath;
}

function readModulePaths(): ModulePaths {
	return {
		truncate: requireModulePath(2),
		shellOutput: requireModulePath(3),
		agent: requireModulePath(4),
		promptTemplates: requireModulePath(5),
		skills: requireModulePath(6),
		systemPrompt: requireModulePath(7),
		image: requireModulePath(8),
		editDiff: requireModulePath(9),
		fileOperations: requireModulePath(10),
		compaction: requireModulePath(11),
		esbuild: requireModulePath(12),
	};
}

async function loadFallbackApi(sourceModulePath: string, esbuildModulePath: string): Promise<TruncateApi> {
	const source = await readFile(sourceModulePath, "utf8");
	const imported = (await import(pathToFileURL(esbuildModulePath).href)) as EsbuildModule;
	const transform = imported.transform ?? imported.default?.transform;
	if (transform === undefined) {
		throw new Error("esbuild transform export is unavailable");
	}
	const transformed = await transform(source, { loader: "ts", format: "cjs", target: "es2022" });
	const module: CommonJsModule = { exports: {} };
	const context = createContext({ module, exports: module.exports });
	runInContext(transformed.code, context, { filename: sourceModulePath });
	return module.exports as unknown as TruncateApi;
}

type OracleApis = {
	normal: TruncateApi;
	fallback: TruncateApi;
	shellOutput: ShellOutputApi;
	harnessTypes: HarnessTypesApi;
	promptTemplates: PromptTemplateApi;
	skillFormatting: SkillFormattingApi;
	systemPrompt: SystemPromptApi;
	image: ImageApi;
	editDiff: EditDiffApi;
	fileOperations: FileOperationsApi;
	compaction: CompactionApi;
};

async function loadApis(): Promise<OracleApis> {
	const paths = readModulePaths();
	const normal = (await import(pathToFileURL(paths.truncate).href)) as TruncateApi;
	const fallback = await loadFallbackApi(paths.truncate, paths.esbuild);
	const shellOutput = (await import(pathToFileURL(paths.shellOutput).href)) as ShellOutputApi;
	const harnessTypes = (await import(pathToFileURL(paths.agent).href)) as HarnessTypesApi;
	const promptTemplates = (await import(pathToFileURL(paths.promptTemplates).href)) as PromptTemplateApi;
	const skillFormatting = (await import(pathToFileURL(paths.skills).href)) as SkillFormattingApi;
	const systemPrompt = (await import(pathToFileURL(paths.systemPrompt).href)) as SystemPromptApi;
	const image = (await import(pathToFileURL(paths.image).href)) as ImageApi;
	const editDiff = (await import(pathToFileURL(paths.editDiff).href)) as EditDiffApi;
	const fileOperations = (await import(pathToFileURL(paths.fileOperations).href)) as FileOperationsApi;
	const compaction = (await import(pathToFileURL(paths.compaction).href)) as CompactionApi;
	return {
		normal,
		fallback,
		shellOutput,
		harnessTypes,
		promptTemplates,
		skillFormatting,
		systemPrompt,
		image,
		editDiff,
		fileOperations,
		compaction,
	};
}

function executeRequestLine(line: string, apis: OracleApis): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify(execute(request, apis))}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(apis: OracleApis): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), apis);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	serveRequests(await loadApis());
}

void main().catch((error: unknown) => {
	process.stderr.write(`truncation oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
