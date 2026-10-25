// Differential oracle for coding-agent compaction.
// argv[2] is the pinned `packages/coding-agent/src` directory; each request names
// the module (relative to it) and an operation handled below.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Api = Record<string, (...args: unknown[]) => unknown> & Record<string, unknown>;
type Request = { id: number; module: string; operation: string; [key: string]: unknown };
type Handler = (api: Api, request: Request, extra?: Api) => unknown;

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
}

function captureGenerationRequest(model: unknown, context: Record<string, unknown>, options: Record<string, unknown>) {
	const messages = (context.messages as Array<Record<string, unknown>>).map((message) => ({ ...message, timestamp: 0 }));
	return { model, context: { ...context, messages }, options: {
		apiKey: options.apiKey, headers: options.headers, env: options.env,
		maxTokens: options.maxTokens, cacheRetention: options.cacheRetention,
	} };
}

const handlers: Record<string, Handler> = {
	generation: async (api, request) => {
		const requests: unknown[] = [];
		const responses = [...request.responses as unknown[]];
		const streamFn = async (model: unknown, context: Record<string, unknown>, options: Record<string, unknown>) => {
			requests.push(captureGenerationRequest(model, context, options));
			if (responses.length === 0) throw new Error("generation fixture exhausted");
			return { result: async () => responses.shift() };
		};
		const options = { apiKey: "fixture-key", headers: { "x-fixture": "yes" }, env: { FIXTURE_ENV: "value" }, streamFn };
		let result: unknown;
		try {
			let value: unknown;
			if (request.kind === "branch") {
				value = await api.generateBranchSummary(request.entries, { ...options, model: request.model, reserveTokens: 1000,
					customInstructions: request.customInstructions, replaceInstructions: request.replaceInstructions });
			} else if (request.kind === "summary") {
				value = await api.generateSummary(request.messages, request.model, 1000, options.apiKey, options.headers,
					undefined, request.customInstructions, request.previousSummary, undefined, streamFn, options.env);
			} else {
				const preparation = request.preparation as Record<string, unknown> & { fileOps: { read: unknown[]; written: unknown[]; edited: unknown[] } };
				value = await api.compact({ ...preparation, fileOps: { read: new Set(preparation.fileOps.read), written: new Set(preparation.fileOps.written), edited: new Set(preparation.fileOps.edited) } },
					request.model, options.apiKey, options.headers, request.customInstructions, undefined, undefined, streamFn, options.env);
				(value as Record<string, unknown>).durationMs = 0;
			}
			result = { ok: true, value };
		} catch (error) { result = { ok: false, error: errorOf(error) }; }
		return { result, requests };
	},
	shouldCompactResults: (api, request) => (request.cases as Array<{ contextTokens: number; contextWindow: number; settings: unknown; threshold?: number }>).map((entry) => {
		try { return { value: api.shouldCompact(entry.contextTokens, entry.contextWindow, entry.settings, entry.threshold) }; }
		catch (error) { return { error: errorOf(error) }; }
	}),
	tokens: (api, request) =>
		(request.usages as unknown[]).map((usage) => api.calculateContextTokens(usage)),
	estimateTokens: (api, request) =>
		(request.messages as unknown[]).map((message) => api.estimateTokens(message)),
	estimateContext: (api, request) => {
		const estimate = api.estimateContextTokens(request.messages) as {
			tokens: number;
			usageTokens: number;
			trailingTokens: number;
			lastUsageIndex: number | null;
		};
		return {
			tokens: estimate.tokens,
			usageTokens: estimate.usageTokens,
			trailingTokens: estimate.trailingTokens,
			lastUsageIndex: estimate.lastUsageIndex,
		};
	},
	shouldCompact: (api, request) =>
		(request.cases as Array<{ contextTokens: number; contextWindow: number; settings: unknown; threshold?: number }>).map(
			(entry) => api.shouldCompact(entry.contextTokens, entry.contextWindow, entry.settings, entry.threshold),
		),
	findCutPoint: (api, request) =>
		api.findCutPoint(
			request.entries,
			request.startIndex as number,
			request.endIndex as number,
			request.keepRecentTokens as number,
		),
	findTurnStartIndex: (api, request) =>
		api.findTurnStartIndex(request.entries, request.entryIndex as number, request.startIndex as number),
	prepareCompaction: (api, request) => {
		const preparation = api.prepareCompaction(request.entries, request.settings) as
			| (Record<string, unknown> & { fileOps: { read: Set<string>; written: Set<string>; edited: Set<string> } })
			| undefined;
		if (!preparation) return null;
		return {
			firstKeptEntryId: preparation.firstKeptEntryId,
			messagesToSummarize: preparation.messagesToSummarize,
			turnPrefixMessages: preparation.turnPrefixMessages,
			isSplitTurn: preparation.isSplitTurn,
			tokensBefore: preparation.tokensBefore,
			previousSummary: preparation.previousSummary ?? null,
			fileOps: {
				read: [...preparation.fileOps.read],
				written: [...preparation.fileOps.written],
				edited: [...preparation.fileOps.edited],
			},
			settings: preparation.settings,
		};
	},
	serializeConversation: (api, request) => api.serializeConversation(request.messages),
	fileLists: (api, request) => {
		const fileOps = api.createFileOps() as {
			read: Set<string>;
			written: Set<string>;
			edited: Set<string>;
		};
		for (const message of request.messages as unknown[]) api.extractFileOpsFromMessage(message, fileOps);
		const { readFiles, modifiedFiles } = api.computeFileLists(fileOps) as {
			readFiles: string[];
			modifiedFiles: string[];
		};
		return {
			read: [...fileOps.read],
			written: [...fileOps.written],
			edited: [...fileOps.edited],
			readFiles,
			modifiedFiles,
			formatted: api.formatFileOperations(readFiles, modifiedFiles),
		};
	},
	prepareBranchEntries: (api, request) => {
		const preparation = api.prepareBranchEntries(request.entries, request.tokenBudget as number) as {
			messages: unknown[];
			fileOps: { read: Set<string>; written: Set<string>; edited: Set<string> };
			totalTokens: number;
		};
		return {
			messages: preparation.messages,
			fileOps: {
				read: [...preparation.fileOps.read],
				written: [...preparation.fileOps.written],
				edited: [...preparation.fileOps.edited],
			},
			totalTokens: preparation.totalTokens,
		};
	},
	collectEntries: (api, request, extra) => {
		if (!extra) throw new Error("collectEntries requires module2");
		const SessionManager = extra.SessionManager as unknown as { open(path: string): unknown };
		const session = SessionManager.open(request.sessionPath as string);
		return api.collectEntriesForBranchSummary(
			session,
			(request.oldLeafId as string | null) ?? null,
			request.targetId as string,
		);
	},
};

async function main(): Promise<void> {
	const sourceRoot = process.argv[2];
	if (!sourceRoot) throw new Error("compaction oracle requires the coding-agent src directory");
	const modules = new Map<string, Api>();
	const load = async (module: string): Promise<Api> => {
		let api = modules.get(module);
		if (!api) {
			api = (await import(pathToFileURL(`${sourceRoot}/${module}`).href)) as Api;
			modules.set(module, api);
		}
		return api;
	};
	for await (const line of createInterface({ input: process.stdin })) {
		let id: number | null = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const handler = handlers[request.operation];
			if (!handler) throw new Error(`unknown operation ${request.operation}`);
			const api = await load(request.module);
			const extra = typeof request.module2 === "string" ? await load(request.module2) : undefined;
			const output = await handler(api, request, extra);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorOf(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`compaction oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
