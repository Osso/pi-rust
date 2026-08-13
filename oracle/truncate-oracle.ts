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
	| HarnessErrorRequest;

type UtilityRequest = Exclude<Request, HarnessErrorRequest>;
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
	}
}

function execute(request: Request, apis: OracleApis): Response {
	try {
		if (request.operation === "harness_error") {
			return executeHarnessError(request, apis.harnessTypes);
		}
		return executeUtilityRequest(
			request,
			apis.normal,
			apis.fallback,
			apis.shellOutput,
			apis.promptTemplates,
			apis.skillFormatting,
			apis.systemPrompt,
			apis.image,
			apis.editDiff,
		);
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

function readModulePaths(): [string, string, string, string, string, string, string, string, string] {
	const truncateModulePath = process.argv[2];
	const shellOutputModulePath = process.argv[3];
	const agentModulePath = process.argv[4];
	const promptTemplateModulePath = process.argv[5];
	const skillsModulePath = process.argv[6];
	const systemPromptModulePath = process.argv[7];
	const imageModulePath = process.argv[8];
	const editDiffModulePath = process.argv[9];
	const esbuildModulePath = process.argv[10];
	if (
		truncateModulePath === undefined ||
		shellOutputModulePath === undefined ||
		agentModulePath === undefined ||
		promptTemplateModulePath === undefined ||
		skillsModulePath === undefined ||
		systemPromptModulePath === undefined ||
		imageModulePath === undefined ||
		editDiffModulePath === undefined ||
		esbuildModulePath === undefined
	) {
		throw new Error(
			"agent utility oracle requires truncate, shell-output, agent-package, prompt-template, skills, system-prompt, image, edit-diff, and esbuild module paths",
		);
	}
	return [
		truncateModulePath,
		shellOutputModulePath,
		agentModulePath,
		promptTemplateModulePath,
		skillsModulePath,
		systemPromptModulePath,
		imageModulePath,
		editDiffModulePath,
		esbuildModulePath,
	];
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
};

async function loadApis(): Promise<OracleApis> {
	const [
		truncateModulePath,
		shellOutputModulePath,
		agentModulePath,
		promptTemplateModulePath,
		skillsModulePath,
		systemPromptModulePath,
		imageModulePath,
		editDiffModulePath,
		esbuildModulePath,
	] = readModulePaths();
	const normal = (await import(pathToFileURL(truncateModulePath).href)) as TruncateApi;
	const fallback = await loadFallbackApi(truncateModulePath, esbuildModulePath);
	const shellOutput = (await import(pathToFileURL(shellOutputModulePath).href)) as ShellOutputApi;
	const harnessTypes = (await import(pathToFileURL(agentModulePath).href)) as HarnessTypesApi;
	const promptTemplates = (await import(pathToFileURL(promptTemplateModulePath).href)) as PromptTemplateApi;
	const skillFormatting = (await import(pathToFileURL(skillsModulePath).href)) as SkillFormattingApi;
	const systemPrompt = (await import(pathToFileURL(systemPromptModulePath).href)) as SystemPromptApi;
	const image = (await import(pathToFileURL(imageModulePath).href)) as ImageApi;
	const editDiff = (await import(pathToFileURL(editDiffModulePath).href)) as EditDiffApi;
	return { normal, fallback, shellOutput, harnessTypes, promptTemplates, skillFormatting, systemPrompt, image, editDiff };
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
