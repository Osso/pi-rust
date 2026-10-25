import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Case = {
	kind: "error" | "value";
	value: unknown;
	missingContext?: boolean;
};
type Execute = (...args: unknown[]) => Promise<unknown>;
type Tool = { execute: Execute };
type Message = { role: string; content: unknown; errorMessage?: string };
type Loop = { runAgentLoop(...args: unknown[]): Promise<Message[]> };
function rejection(input: Case): unknown {
	if (input.kind === "value") return input.value;
	if (typeof input.value !== "string")
		throw new Error("Error fixture message required");
	return Object.assign(new TypeError(input.value), { code: "E_CUSTOM" });
}
function snapshot(error: unknown) {
	return error instanceof Error
		? {
				kind: "error",
				value: {
					name: error.name,
					message: error.message,
					...Object.fromEntries(Object.entries(error)),
				},
			}
		: { kind: "value", value: error };
}
function response(content: unknown[], stopReason: string) {
	return {
		role: "assistant",
		content,
		api: "openai-responses",
		provider: "openai",
		model: "mock",
		usage: {
			input: 0,
			output: 0,
			cacheRead: 0,
			cacheWrite: 0,
			totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
		},
		stopReason,
		timestamp: 1000,
	};
}
function scriptedRequest(params: unknown) {
	let turn = 0;
	const stream = () => {
		const message =
			turn++ === 0
				? response(
						[
							{
								type: "toolCall",
								id: "call",
								name: "ask_questions",
								arguments: params,
							},
						],
						"toolUse",
					)
				: response([], "stop");
		return {
			async *[Symbol.asyncIterator]() {
				yield { type: "done", reason: message.stopReason, message };
			},
			async result() {
				return message;
			},
		};
	};
	const model = {
		id: "mock",
		name: "mock",
		api: "openai-responses",
		provider: "openai",
		baseUrl: "https://example.invalid",
		reasoning: false,
		input: ["text"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 8192,
		maxTokens: 2048,
	};
	return { model, stream };
}
async function runtimeResult(loop: Loop, tool: Tool, params: unknown) {
	const { model, stream } = scriptedRequest(params);
	const messages = await loop.runAgentLoop(
		[{ role: "user", content: "Ask", timestamp: 1000 }],
		{ systemPrompt: "", messages: [], tools: [tool] },
		{ model, convertToLlm: (messages: unknown[]) => messages },
		async () => {},
		undefined,
		stream,
	);
	return messages
		.filter((message) => message.role === "toolResult")
		.map((message) => message.content);
}
async function promptProbe(source: string, tool: Tool, params: unknown) {
	const api = (await import(
		pathToFileURL(join(dirname(source), "../../../../agent-core/src/agent.ts"))
			.href
	)) as {
		Agent: new (
			options: unknown,
		) => {
			prompt(input: string): Promise<void>;
			state: { messages: Message[]; isStreaming: boolean };
		};
	};
	const { model, stream } = scriptedRequest(params);
	const agent = new api.Agent({
		initialState: { model, tools: [tool] },
		convertToLlm: (messages: unknown[]) => messages,
		streamFn: stream,
	});
	await agent.prompt("Ask");
	return {
		resolved: true,
		isStreaming: agent.state.isStreaming,
		errors: agent.state.messages.flatMap((message) =>
			message.errorMessage === undefined ? [] : [message.errorMessage],
		),
		toolResults: agent.state.messages.filter(
			(message) => message.role === "toolResult",
		).length,
	};
}
void runOracle(async (module, request) => {
	const source = process.argv[2];
	if (!source) throw new Error("frozen module required");
	const loop = (await import(
		pathToFileURL(
			join(dirname(source), "../../../../agent-core/src/agent-loop.ts"),
		).href
	)) as Loop;
	const outputs = [];
	for (const item of requireCases(request)) {
		const input = item as Case;
		const reason = rejection(input);
		const context = input.missingContext
			? undefined
			: {
					hasUI: true,
					mode: "tui",
					ui: {
						custom: async () => {
							throw reason;
						},
					},
				};
		const definition = (
			module as { createAskQuestionsToolDefinition(): Tool }
		).createAskQuestionsToolDefinition();
		const params = { blocking: true, questions: [{ question: "Question" }] };
		let direct: unknown;
		try {
			await definition.execute("call", params, undefined, undefined, context);
			throw new Error("expected rejection");
		} catch (error) {
			direct = snapshot(error);
		}
		const tool = {
			...definition,
			execute: (id: unknown, args: unknown, signal: unknown, update: unknown) =>
				definition.execute(id, args, signal, update, context),
		};
		outputs.push(
			request.coercionProbe === true
				? { direct, prompt: await promptProbe(source, tool, params) }
				: { direct, runtime: await runtimeResult(loop, tool, params) },
		);
	}
	return outputs;
}).catch((error: unknown) => {
	process.stderr.write(
		`question custom rejection oracle failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
