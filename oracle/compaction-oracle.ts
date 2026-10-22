import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

import {
	createModels,
	fauxAssistantMessage,
	fauxProvider,
	type FauxProviderHandle,
	type Model,
	type Models,
} from "@earendil-works/pi-ai";

type RequestId = string | number | null;

type Api = Record<string, any>;

type Call = {
	prompt: string;
	maxTokens: number | null;
	reasoning: string | null;
	cacheRetention: string | null;
};

type CaseResult =
	| { name: string; ok: true; output: unknown }
	| { name: string; ok: false; error: unknown };

type Response =
	| { id: RequestId; ok: true; output: { results: CaseResult[] } }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

let compactionApi: Api;
let branchApi: Api;
let messagesApi: Api;
let sessionApi: Api;
let utilsApi: Api;

function normalizeError(error: unknown): {
	name: string;
	message: string;
	code?: string;
} {
	const normalized: { name: string; message: string; code?: string } = {
		name: error instanceof Error ? error.name : "UnknownError",
		message: error instanceof Error ? error.message : String(error),
	};
	const code = (error as { code?: unknown } | null)?.code;
	if (typeof code === "string") normalized.code = code;
	return normalized;
}

function ordered(values: Set<string>): string[] {
	return [...values];
}

function fileOpsJson(fileOps: any): unknown {
	return {
		read: ordered(fileOps.read),
		written: ordered(fileOps.written),
		edited: ordered(fileOps.edited),
	};
}

function fileOpsFromJson(value: any): any {
	return {
		read: new Set<string>(value?.read ?? []),
		written: new Set<string>(value?.written ?? []),
		edited: new Set<string>(value?.edited ?? []),
	};
}

function preparationJson(preparation: any): unknown {
	if (preparation === undefined) return null;
	const value: any = {
		messagesToSummarize: preparation.messagesToSummarize,
		turnPrefixMessages: preparation.turnPrefixMessages,
		firstKeptEntryId: preparation.firstKeptEntryId,
		isSplitTurn: preparation.isSplitTurn,
		tokensBefore: preparation.tokensBefore,
		fileOps: fileOpsJson(preparation.fileOps),
		settings: preparation.settings,
	};
	if (preparation.previousSummary !== undefined)
		value.previousSummary = preparation.previousSummary;
	return value;
}

function firstUserText(context: any): string {
	const message = context.messages[0];
	if (!message || message.role !== "user") return "";
	if (typeof message.content === "string") return message.content;
	return message.content
		.filter((block: any) => block.type === "text")
		.map((block: any) => block.text)
		.join("");
}

let fauxCount = 0;

function createModel(spec: any): {
	faux: FauxProviderHandle;
	models: Models;
	model: Model<any>;
} {
	const faux = fauxProvider({
		provider: `faux-oracle-${++fauxCount}`,
		models: [
			{
				id: spec?.id ?? "oracle-model",
				reasoning: spec?.reasoning ?? false,
				contextWindow: spec?.contextWindow ?? 200000,
				maxTokens: spec?.maxTokens ?? 8192,
			},
		],
	});
	const models = createModels();
	models.setProvider(faux.provider);
	return { faux, models, model: faux.getModel() as Model<any> };
}

function queueResponses(
	faux: FauxProviderHandle,
	responses: any[],
	calls: Call[],
): void {
	faux.setResponses(
		responses.map((response) => (context: any, options: any) => {
			calls.push({
				prompt: firstUserText(context),
				maxTokens: options?.maxTokens ?? null,
				reasoning: options?.reasoning ?? null,
				cacheRetention: options?.cacheRetention ?? null,
			});
			return fauxAssistantMessage(response.text ?? "", {
				stopReason: response.stopReason,
				errorMessage: response.errorMessage,
			});
		}),
	);
}

function unwrap(result: any): any {
	if (result && typeof result === "object" && "ok" in result) {
		if (!result.ok) throw result.error;
		return result.value;
	}
	return result;
}

function executeSyncCase(operation: string, item: any): unknown {
	switch (operation) {
		case "estimate_tokens":
			return compactionApi.estimateTokens(item.message);
		case "calculate_context_tokens":
			return compactionApi.calculateContextTokens(item.usage);
		case "get_last_assistant_usage":
			return compactionApi.getLastAssistantUsage(item.entries) ?? null;
		case "estimate_context_tokens":
			return compactionApi.estimateContextTokens(item.messages);
		case "find_cut_point":
			return compactionApi.findCutPoint(
				item.entries,
				item.startIndex,
				item.endIndex,
				item.keepRecentTokens,
			);
		case "find_turn_start_index":
			return compactionApi.findTurnStartIndex(
				item.entries,
				item.entryIndex,
				item.startIndex,
			);
		case "prepare_compaction":
			return preparationJson(
				unwrap(
					compactionApi.prepareCompaction(item.pathEntries, item.settings),
				),
			);
		case "serialize_conversation":
			return compactionApi.serializeConversation(
				messagesApi.convertToLlm(item.messages),
			);
		case "prepare_branch_entries": {
			const preparation = branchApi.prepareBranchEntries(
				item.entries,
				item.tokenBudget,
			);
			return {
				messages: preparation.messages,
				fileOps: fileOpsJson(preparation.fileOps),
				totalTokens: preparation.totalTokens,
			};
		}
		case "compute_file_lists":
			return utilsApi.computeFileLists(fileOpsFromJson(item.fileOps));
		case "format_file_operations":
			return utilsApi.formatFileOperations(item.readFiles, item.modifiedFiles);
		default:
			throw new Error(`unsupported compaction operation: ${operation}`);
	}
}

async function executeAsyncCase(
	operation: string,
	item: any,
): Promise<unknown> {
	switch (operation) {
		case "generate_summary": {
			const { faux, models, model } = createModel(item.model);
			const calls: Call[] = [];
			queueResponses(faux, [item.response], calls);
			const options: any = {
				currentMessages: item.messages,
				models,
				model,
				reserveTokens: item.reserveTokens,
			};
			if (item.customInstructions !== undefined)
				options.customInstructions = item.customInstructions;
			if (item.previousSummary !== undefined)
				options.previousSummary = item.previousSummary;
			if (item.thinkingLevel !== undefined)
				options.thinkingLevel = item.thinkingLevel;
			const result = unwrap(
				await compactionApi.generateSummary(
					item.messages,
					models,
					model,
					item.reserveTokens,
					undefined,
					item.customInstructions,
					item.previousSummary,
					item.thinkingLevel,
				),
			);
			return { text: result, calls };
		}
		case "compact": {
			const { faux, models, model } = createModel(item.model);
			const calls: Call[] = [];
			queueResponses(faux, item.responses, calls);
			const preparation = {
				...item.preparation,
				fileOps: fileOpsFromJson(item.preparation.fileOps),
			};
			const result = unwrap(
				await compactionApi.compact(
					preparation,
					models,
					model,
					item.customInstructions,
					undefined,
					item.thinkingLevel,
				),
			);
			return {
				summary: result.summary,
				tokensBefore: result.tokensBefore,
				firstKeptEntryId: result.firstKeptEntryId,
				details: result.details ?? null,
				calls,
			};
		}
		case "generate_branch_summary": {
			const { faux, models, model } = createModel(item.model);
			const calls: Call[] = [];
			queueResponses(faux, item.responses, calls);
			const result = unwrap(
				await branchApi.generateBranchSummary(item.entries, {
					models,
					model,
					signal: new AbortController().signal,
					customInstructions: item.customInstructions,
					replaceInstructions: item.replaceInstructions,
					reserveTokens: item.reserveTokens,
				}),
			);
			return {
				summary: result.summary,
				readFiles: result.readFiles,
				modifiedFiles: result.modifiedFiles,
				calls,
			};
		}
		case "collect_entries_for_branch_summary": {
			const { InMemorySessionStorage } = await importApi(
				join(dirname(process.argv[5]), "memory-storage.ts"),
			);
			const storage = new InMemorySessionStorage({ entries: item.entries });
			const session = new sessionApi.Session(storage);
			const result = unwrap(
				await branchApi.collectEntriesForBranchSummary(
					session,
					item.oldLeafId,
					item.targetId,
				),
			);
			return {
				entryIds: result.entries.map((entry: any) => entry.id),
				commonAncestorId: result.commonAncestorId,
			};
		}
		default:
			throw new Error(`unsupported compaction operation: ${operation}`);
	}
}

async function executeCase(
	operation: string,
	item: any,
	asyncOperation: boolean,
): Promise<CaseResult> {
	try {
		const output = asyncOperation
			? await executeAsyncCase(operation, item)
			: executeSyncCase(operation, item);
		return { name: item.name ?? "", ok: true, output };
	} catch (error) {
		return { name: item.name ?? "", ok: false, error: normalizeError(error) };
	}
}

async function executeRequest(request: any): Promise<Response> {
	try {
		const asyncOperation =
			request.operation === "generate_summary" ||
			request.operation === "compact" ||
			request.operation === "generate_branch_summary" ||
			request.operation === "collect_entries_for_branch_summary";
		const results: CaseResult[] = [];
		for (const item of request.cases ?? []) {
			results.push(await executeCase(request.operation, item, asyncOperation));
		}
		return { id: request.id ?? null, ok: true, output: { results } };
	} catch (error) {
		return { id: request.id ?? null, ok: false, error: normalizeError(error) };
	}
}

async function importApi(modulePath: string): Promise<Api> {
	return (await import(pathToFileURL(modulePath).href)) as Api;
}

async function main(): Promise<void> {
	const [compactionPath, branchPath, messagesPath, sessionPath] =
		process.argv.slice(2);
	if (!compactionPath || !branchPath || !messagesPath || !sessionPath) {
		throw new Error(
			"compaction oracle requires compaction, branch, messages, and session module paths",
		);
	}
	compactionApi = await importApi(compactionPath);
	branchApi = await importApi(branchPath);
	messagesApi = await importApi(messagesPath);
	sessionApi = await importApi(sessionPath);
	utilsApi = await importApi(join(dirname(compactionPath), "utils.ts"));

	const reader = createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of reader) {
		if (!line.trim()) continue;
		let request: any;
		try {
			request = JSON.parse(line);
		} catch {
			continue;
		}
		process.stdout.write(`${JSON.stringify(await executeRequest(request))}\n`);
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(
		`compaction oracle startup failed: ${normalizeError(error).message}\n`,
	);
	process.exitCode = 1;
});
