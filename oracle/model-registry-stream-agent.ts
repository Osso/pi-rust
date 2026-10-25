import type { ModelRegistry } from "@pi-oracle-source/coding-agent/src/core/model-registry.ts";

function snapshotRejection(error: unknown): unknown {
	if (!(error instanceof Error)) return { kind: "value", value: error };
	return {
		kind: "error",
		value: {
			name: error.name,
			message: error.message,
			...Object.fromEntries(
				Object.entries(error).filter(([key]) => key !== "stack"),
			),
		},
	};
}

export async function runRegisteredAgentThrow(
	source: URL,
	registry: ModelRegistry,
	request: Record<string, unknown>,
	thrown: unknown,
) {
	const provider =
		request.consumer === "loop" || request.consumer === "continue"
			? `registry-agent-${request.consumer}-throw`
			: "registry-agent-throw";
	let callbackCount = 0;
	registry.registerProvider(provider, {
		...(request.config as Record<string, unknown>),
		api: `${provider}-api`,
		apiKey: "config-key",
		streamSimple: () => {
			callbackCount++;
			throw thrown;
		},
	});
	if (request.consumer === "loop" || request.consumer === "continue") {
		const { agentLoop, agentLoopContinue } = await import(
			new URL("../../../agent-core/src/agent-loop.ts", source).href
		);
		const before = Date.now();
		const prompt = {
			role: "user",
			content: [{ type: "text", text: "prompt" }],
			timestamp: Date.now(),
		};
		const context = { systemPrompt: "", messages: [], tools: [] };
		const config = {
			model: registry.find(provider, "one"),
			apiKey: "stream-key",
			convertToLlm: (messages: unknown[]) => messages,
		};
		const stream =
			request.consumer === "loop"
				? agentLoop([prompt], context, config)
				: agentLoopContinue({ ...context, messages: [prompt] }, config);
		const events: unknown[] = [];
		let iteration: unknown = null;
		try {
			for await (const event of stream) events.push(event);
		} catch (error) {
			iteration = snapshotRejection(error);
		}
		let result: unknown = null;
		try {
			await stream.result();
		} catch (error) {
			result = snapshotRejection(error);
		}
		registry.unregisterProvider(provider);
		return {
			clock: { before, after: Date.now() },
			snapshot: { callbackCount, events, iteration, result },
		};
	}
	const { Agent } = await import(
		new URL("../../../agent-core/src/agent.ts", source).href
	);
	const agent = new Agent({
		initialState: { model: registry.find(provider, "one") },
		getApiKey: async () => "stream-key",
	});
	const events: unknown[] = [];
	agent.subscribe((event: unknown) => {
		events.push(event);
	});
	const before = Date.now();
	let result: unknown = { kind: "fulfilled" };
	try {
		await agent.prompt("prompt");
	} catch (error) {
		result =
			error instanceof Error
				? {
						kind: "rejected",
						value: { name: error.name, message: error.message },
					}
				: { kind: "rejected", value: error };
	}
	const state = agent.state;
	registry.unregisterProvider(provider);
	return {
		clock: { before, after: Date.now() },
		snapshot: {
			callbackCount,
			result,
			events,
			state: {
				messages: state.messages,
				isStreaming: state.isStreaming,
				isModelRequestActive: state.isModelRequestActive,
				streamingMessage: state.streamingMessage ?? null,
				pendingToolCalls: [...state.pendingToolCalls],
				errorMessage: state.errorMessage ?? null,
			},
		},
	};
}
