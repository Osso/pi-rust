import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type ScenarioRequest = {
	id: RequestId;
	scenario: string;
};

const usage = {
	input: 0,
	output: 0,
	cacheRead: 0,
	cacheWrite: 0,
	totalTokens: 0,
	cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
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

function assistant(content: any[], stopReason = "stop"): any {
	return {
		role: "assistant",
		content,
		api: "openai-responses",
		provider: "openai",
		model: "mock",
		usage,
		stopReason,
		timestamp: 1000,
	};
}

function user(text: string): any {
	return { role: "user", content: text, timestamp: 1000 };
}

function userBlocks(text: string): any {
	return { role: "user", content: [{ type: "text", text }], timestamp: 1000 };
}

function textBlock(text: string): any {
	return { type: "text", text };
}

function call(id: string, name: string, args: any): any {
	return { type: "toolCall", id, name, arguments: args };
}

function result(value: string, extra: any = {}): any {
	return { content: [textBlock(value)], details: { value }, ...extra };
}

function snapshot(value: any): any {
	return JSON.parse(JSON.stringify(value));
}

function normalize(value: any): any {
	if (Array.isArray(value)) return value.map(normalize);
	if (value && typeof value === "object") {
		const output: any = {};
		for (const [key, entry] of Object.entries(value)) {
			if (key === "timestamp" || key === "startedAt" || key === "finishedAt") {
				output[key] = 0;
			} else {
				output[key] = normalize(entry);
			}
		}
		return output;
	}
	return value;
}

function done(reason: string, message: any): any {
	return { type: "done", reason, message };
}

async function collect(
	stream: any,
): Promise<{ events: any[]; messages: any[] }> {
	const events: any[] = [];
	for await (const event of stream) {
		events.push(normalize(snapshot(event)));
	}
	const messages = normalize(snapshot(await stream.result()));
	return { events, messages };
}

async function collectOutcome(
	stream: any,
): Promise<{ events: any[]; messages?: any; error?: string }> {
	const events: any[] = [];
	let error: string | undefined;
	try {
		for await (const event of stream) {
			events.push(normalize(snapshot(event)));
		}
	} catch (failure) {
		error = failure instanceof Error ? failure.message : String(failure);
	}
	if (!error) {
		try {
			const messages = normalize(snapshot(await stream.result()));
			return { events, messages };
		} catch (failure) {
			error = failure instanceof Error ? failure.message : String(failure);
		}
	}
	return { events, error };
}

async function executeScenario(api: any, scenario: string): Promise<any> {
	const requests: any[] = [];
	const scriptStream = (scripts: any[]) => {
		let index = 0;
		return (_model: any, context: any, _options: any) => {
			requests.push(
				normalize(
					snapshot({
						systemPrompt: context.systemPrompt,
						messages: context.messages,
					}),
				),
			);
			const script = scripts[index++];
			if (!script) throw new Error("Unexpected stream call");
			const { AssistantMessageEventStream } = api.ai;
			const stream = new AssistantMessageEventStream();
			queueMicrotask(() => {
				for (const event of script()) stream.push(event);
			});
			return stream;
		};
	};
	const config = (overrides: any = {}) => ({
		model,
		convertToLlm: (messages: any[]) =>
			messages.filter((message) =>
				["user", "assistant", "toolResult"].includes(message.role),
			),
		...overrides,
	});
	const context = (messages: any[], tools: any[] = [], systemPrompt = "") => ({
		systemPrompt,
		messages,
		tools,
	});
	const runLoopWithThrowingEmit = async (
		throwOn: string,
		scripts: any[],
		tools: any[] = [],
	) => {
		const events: any[] = [];
		const emit = async (event: any) => {
			events.push(normalize(snapshot(event)));
			if (event.type === throwOn)
				throw new Error(`listener exploded on ${event.type}`);
		};
		try {
			const messages = await api.runAgentLoop(
				[user("Hello")],
				context([], tools),
				config(),
				emit,
				undefined,
				scriptStream(scripts),
			);
			return { events, messages: normalize(snapshot(messages)) };
		} catch (failure) {
			return {
				events,
				error: failure instanceof Error ? failure.message : String(failure),
			};
		}
	};
	const runAgentWithThrowingListener = async (
		throwOn: string,
		scripts: any[],
		tools: any[] = [],
	) => {
		const agent = new api.Agent({
			initialState: { tools },
			streamFn: scriptStream(scripts),
		});
		const events: any[] = [];
		agent.subscribe((event: any) => {
			events.push(normalize(snapshot(event)));
			if (event.type === throwOn)
				throw new Error(`listener exploded on ${event.type}`);
		});
		let error: string | undefined;
		try {
			await agent.prompt("Hello");
		} catch (failure) {
			error = failure instanceof Error ? failure.message : String(failure);
		}
		const output: any = {
			events,
			messages: normalize(snapshot(agent.state.messages)),
			state: agentState(agent),
			requests,
		};
		if (error !== undefined) output.error = error;
		return output;
	};

	switch (scenario) {
		case "text": {
			const stream = api.agentLoop(
				[user("Hello")],
				context([]),
				config(),
				undefined,
				scriptStream([
					() => [done("stop", assistant([textBlock("Hi there!")]))],
				]),
			);
			const collected = await collect(stream);
			return { ...collected, requests };
		}
		case "tools": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`),
			};
			const stream = api.agentLoop(
				[user("echo something")],
				context([], [echo]),
				config(),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "tool_error": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async () => {
					throw new Error("tool exploded");
				},
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config(),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "parallel_order": {
			let releaseFirst: (() => void) | undefined;
			const firstDone = new Promise<void>((resolve) => {
				releaseFirst = resolve;
			});
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) => {
					if (params.value === "first") await firstDone;
					if (params.value === "second") releaseFirst?.();
					return result(`echoed: ${params.value}`);
				},
			};
			const stream = api.agentLoop(
				[user("echo both")],
				context([], [echo]),
				config({ toolExecution: "parallel" }),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[
									call("tool-1", "echo", { value: "first" }),
									call("tool-2", "echo", { value: "second" }),
								],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "sequential_config":
		case "sequential_tool_mode": {
			const echo: any = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`),
			};
			if (scenario === "sequential_tool_mode")
				echo.executionMode = "sequential";
			const overrides =
				scenario === "sequential_config" ? { toolExecution: "sequential" } : {};
			const stream = api.agentLoop(
				[user("echo both")],
				context([], [echo]),
				config(overrides),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[
									call("tool-1", "echo", { value: "first" }),
									call("tool-2", "echo", { value: "second" }),
								],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "blocked_terminate": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async () => result("should not execute"),
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config({
					beforeToolCall: async () => ({
						block: true,
						reason: "Blocked by policy",
						terminate: true,
					}),
				}),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("should not run")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "before_mutation": {
			const seen: any[] = [];
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) => {
					seen.push(snapshot(params));
					return result(`echoed: ${params.value}`);
				},
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config({
					beforeToolCall: async ({ args }: any) => {
						args.value = 123;
					},
				}),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			const collected = await collect(stream);
			return { ...collected, requests, toolArgs: seen };
		}
		case "terminate_all": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`, { terminate: true }),
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config(),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "terminate_mixed": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`, {
						terminate: params.value === "first",
					}),
			};
			const stream = api.agentLoop(
				[user("echo both")],
				context([], [echo]),
				config({ toolExecution: "parallel" }),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[
									call("tool-1", "echo", { value: "first" }),
									call("tool-2", "echo", { value: "second" }),
								],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "after_tool_call": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`),
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config({
					afterToolCall: async () => ({
						content: [textBlock("overridden")],
						details: { overridden: true },
						isError: true,
						terminate: true,
					}),
				}),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "prepare_arguments_error": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				prepareArguments: () => {
					throw new Error("prepareArguments exploded");
				},
				execute: async () => result("should not run"),
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config(),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "prepare_execution": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				prepareExecution: async () => ({ agentId: "known-agent" }),
				execute: async (
					_id: string,
					params: any,
					_signal: any,
					_onUpdate: any,
					execution: any,
				) => result(`${params.value}:${execution?.agentId}`),
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config(),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("provider-call", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "steering_error":
		case "follow_up_error": {
			const events: any[] = [];
			const hook = async () => {
				throw new Error("queue hook failed");
			};
			const overrides =
				scenario === "steering_error"
					? { getSteeringMessages: hook }
					: { getFollowUpMessages: hook };
			try {
				await api.runAgentLoop(
					[user("work")],
					context([]),
					config(overrides),
					async (event: any) => {
						events.push(normalize(snapshot(event)));
					},
					undefined,
					scriptStream([
						() => [done("stop", assistant([textBlock("answer")]))],
					]),
				);
				throw new Error("queue rejection must escape");
			} catch (error) {
				return {
					events,
					error: error instanceof Error ? error.message : String(error),
					requests,
				};
			}
		}
		case "steering": {
			let polls = 0;
			const stream = api.agentLoop(
				[user("work")],
				context([]),
				config({
					getSteeringMessages: async () => {
						polls += 1;
						return polls === 2 ? [user("interrupt")] : [];
					},
				}),
				undefined,
				scriptStream([
					() => [done("stop", assistant([textBlock("first")]))],
					() => [done("stop", assistant([textBlock("second")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "follow_up": {
			let polls = 0;
			const stream = api.agentLoop(
				[user("work")],
				context([]),
				config({
					getFollowUpMessages: async () => {
						polls += 1;
						return polls === 1 ? [user("follow up")] : [];
					},
				}),
				undefined,
				scriptStream([
					() => [done("stop", assistant([textBlock("first")]))],
					() => [done("stop", assistant([textBlock("second")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "error_stop": {
			const stream = api.agentLoop(
				[user("work")],
				context([]),
				config(),
				undefined,
				scriptStream([
					() => [done("stop", assistant([textBlock("")], "error"))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "length_stop": {
			const stream = api.agentLoop(
				[user("work")],
				context([]),
				config(),
				undefined,
				scriptStream([
					() => [done("length", assistant([textBlock("partial")], "length"))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "prepare_next_turn": {
			let prepared = false;
			const stream = api.agentLoop(
				[user("work")],
				context([], [], "first prompt"),
				config({
					prepareNextTurn: async ({ context: current }: any) => {
						if (prepared) return undefined;
						prepared = true;
						return {
							context: {
								systemPrompt: "second prompt",
								messages: current.messages.slice(),
								tools: current.tools,
							},
						};
					},
				}),
				undefined,
				scriptStream([
					() => [done("stop", assistant([textBlock("first")]))],
					() => [done("stop", assistant([textBlock("second")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "should_stop": {
			let steeringPolls = 0;
			let followUpPolls = 0;
			const stream = api.agentLoop(
				[user("work")],
				context([]),
				config({
					getSteeringMessages: async () => {
						steeringPolls += 1;
						return [];
					},
					getFollowUpMessages: async () => {
						followUpPolls += 1;
						return [user("stays queued")];
					},
					shouldStopAfterTurn: async () => true,
				}),
				undefined,
				scriptStream([() => [done("stop", assistant([textBlock("first")]))]]),
			);
			const collected = await collect(stream);
			return {
				...collected,
				requests,
				polls: { steeringPolls, followUpPolls },
			};
		}
		case "end_turn": {
			const endTurn = {
				name: "end_turn",
				label: "End Turn",
				description: "End the current turn",
				parameters: {
					type: "object",
					properties: { reason: { type: "string" } },
					required: ["reason"],
				},
				execute: async (_id: string, params: any) =>
					result(`Ended: ${params.reason}`, { terminate: true }),
			};
			const stream = api.agentLoop(
				[user("work")],
				context([], [endTurn]),
				config(),
				undefined,
				scriptStream([
					() => [done("stop", assistant([textBlock("intermediate update")]))],
					() => [
						done(
							"toolUse",
							assistant(
								[
									call("end-1", "end_turn", {
										reason: "Finished requested work",
									}),
								],
								"toolUse",
							),
						),
					],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "continue_restored": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`),
			};
			const unfinished = assistant(
				[call("tool-restored", "echo", { value: "restored" })],
				"toolUse",
			);
			const stream = api.agentLoopContinue(
				context([user("Echo this"), unfinished], [echo]),
				config(),
				undefined,
				scriptStream([() => [done("stop", assistant([textBlock("Done")]))]]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "continue_partial_results": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) => result(params.value),
			};
			const interrupted = assistant(
				[
					call("tool-complete", "echo", { value: "complete" }),
					call("tool-pending", "echo", { value: "pending" }),
				],
				"toolUse",
			);
			const earlier = {
				role: "toolResult",
				toolCallId: "tool-complete",
				toolName: "echo",
				content: [textBlock("complete")],
				isError: false,
				timestamp: 1000,
			};
			const stream = api.agentLoopContinue(
				context([user("Echo twice"), interrupted, earlier], [echo]),
				config(),
				undefined,
				scriptStream([() => [done("stop", assistant([textBlock("Done")]))]]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "convert_custom": {
			const custom = {
				role: "notification",
				text: "This is a notification",
				timestamp: 1000,
			};
			const stream = api.agentLoop(
				[user("Hello")],
				context([custom]),
				config({
					convertToLlm: (messages: any[]) =>
						messages.filter((message) => message.role !== "notification"),
				}),
				undefined,
				scriptStream([
					() => [done("stop", assistant([textBlock("Response")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "parallel_update": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (
					_id: string,
					params: any,
					_signal: any,
					onUpdate: any,
				) => {
					onUpdate({
						content: [textBlock("partial")],
						details: { status: "partial" },
					});
					return result(`echoed: ${params.value}`);
				},
			};
			const stream = api.agentLoop(
				[user("echo")],
				context([], [echo]),
				config(),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		case "agent_text": {
			const agent = new api.Agent({
				streamFn: scriptStream([
					() => [done("stop", assistant([textBlock("Hi")]))],
				]),
			});
			const events: any[] = [];
			agent.subscribe((event: any) => {
				events.push(normalize(snapshot(event)));
			});
			await agent.prompt("Hello");
			return {
				events,
				messages: normalize(snapshot(agent.state.messages)),
				state: agentState(agent),
				requests,
			};
		}
		case "abort_stream": {
			const controller = new AbortController();
			const streamFn = (_model: any, context: any, _options: any) => {
				requests.push(
					normalize(
						snapshot({
							systemPrompt: context.systemPrompt,
							messages: context.messages,
						}),
					),
				);
				const stream = new api.ai.AssistantMessageEventStream();
				queueMicrotask(() => {
					stream.push({ type: "start", partial: assistant([], "stop") });
				});
				return stream;
			};
			const stream = api.agentLoop(
				[user("work")],
				context([]),
				config(),
				controller.signal,
				streamFn,
			);
			setTimeout(() => controller.abort(), 5);
			return { ...(await collectOutcome(stream)), requests };
		}
		case "abort_tool": {
			let markToolStarted = () => {};
			const toolStarted = new Promise<void>((resolve) => {
				markToolStarted = resolve;
			});
			const tool = {
				name: "stuck_tool",
				label: "Stuck Tool",
				description: "Never settles",
				parameters: { type: "object", properties: {} },
				execute: async () => {
					markToolStarted();
					return await new Promise<never>(() => undefined);
				},
			};
			const agent = new api.Agent({
				initialState: { tools: [tool] },
				streamFn: scriptStream([
					() => [
						done(
							"toolUse",
							assistant([call("call-1", "stuck_tool", {})], "toolUse"),
						),
					],
				]),
			});
			const events: any[] = [];
			agent.subscribe((event: any) => {
				events.push(normalize(snapshot(event)));
			});
			const prompt = agent.prompt("run stuck tool");
			await toolStarted;
			agent.abort();
			await prompt;
			return {
				events,
				messages: normalize(snapshot(agent.state.messages)),
				state: agentState(agent),
				requests,
			};
		}
		case "agent_continue_queue": {
			const agent = new api.Agent({
				streamFn: scriptStream([
					() => [done("stop", assistant([textBlock("Processed")]))],
				]),
			});
			agent.state.messages = [
				userBlocks("Initial"),
				assistant([textBlock("Initial response")]),
			];
			agent.followUp(userBlocks("Queued follow-up"));
			const events: any[] = [];
			agent.subscribe((event: any) => {
				events.push(normalize(snapshot(event)));
			});
			await agent.continue();
			return {
				events,
				messages: normalize(snapshot(agent.state.messages)),
				state: agentState(agent),
				requests,
			};
		}
		case "listener_message_end_loop":
			return runLoopWithThrowingEmit("message_end", [
				() => [done("stop", assistant([textBlock("Hi there!")]))],
			]);
		case "listener_agent_end_loop":
			return runLoopWithThrowingEmit("agent_end", [
				() => [done("stop", assistant([textBlock("Hi there!")]))],
			]);
		case "listener_tool_execution_end_loop": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`),
			};
			return runLoopWithThrowingEmit(
				"tool_execution_end",
				[
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				],
				[echo],
			);
		}
		case "listener_message_end_agent":
			return runAgentWithThrowingListener("message_end", [
				() => [done("stop", assistant([textBlock("Hi there!")]))],
			]);
		case "listener_agent_end_agent":
			return runAgentWithThrowingListener("agent_end", [
				() => [done("stop", assistant([textBlock("Hi there!")]))],
			]);
		case "listener_tool_execution_end_agent": {
			const echo = {
				name: "echo",
				label: "Echo",
				description: "Echo tool",
				parameters: {
					type: "object",
					properties: { value: { type: "string" } },
					required: ["value"],
				},
				execute: async (_id: string, params: any) =>
					result(`echoed: ${params.value}`),
			};
			return runAgentWithThrowingListener(
				"tool_execution_end",
				[
					() => [
						done(
							"toolUse",
							assistant(
								[call("tool-1", "echo", { value: "hello" })],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				],
				[echo],
			);
		}
		case "tool_result_cap": {
			const bigText = (mode: string) => {
				if (mode === "lines") {
					return Array.from({ length: 2001 }, (_, index) =>
						String.fromCharCode(97 + (index % 26)),
					).join("\n");
				}
				if (mode === "single") return "a".repeat(20000);
				return "😀".repeat(3000);
			};
			const bigDetails = (mode: string) => ({
				"2": 2.0,
				"1": 1.0,
				value: mode === "lines" ? "y".repeat(20000) : bigText(mode),
			});
			const big = {
				name: "big",
				label: "Big",
				description: "Big tool",
				parameters: {
					type: "object",
					properties: { mode: { type: "string" } },
					required: ["mode"],
				},
				execute: async (
					_id: string,
					params: any,
					_signal: any,
					onUpdate: any,
				) => {
					const text = bigText(params.mode);
					const details = bigDetails(params.mode);
					if (params.mode === "lines") {
						onUpdate({
							content: [textBlock(text)],
							details: { value: "z".repeat(20000) },
						});
					}
					return { content: [textBlock(text)], details };
				},
			};
			const stream = api.agentLoop(
				[user("cap results")],
				context([], [big]),
				config({ toolExecution: "sequential" }),
				undefined,
				scriptStream([
					() => [
						done(
							"toolUse",
							assistant(
								[
									call("cap-lines", "big", { mode: "lines" }),
									call("cap-single", "big", { mode: "single" }),
									call("cap-emoji", "big", { mode: "emoji" }),
								],
								"toolUse",
							),
						),
					],
					() => [done("stop", assistant([textBlock("done")]))],
				]),
			);
			return { ...(await collect(stream)), requests };
		}
		default:
			throw new Error(`Unknown scenario: ${scenario}`);
	}
}

function agentState(agent: any): any {
	return {
		isStreaming: agent.state.isStreaming,
		isModelRequestActive: agent.state.isModelRequestActive,
		streamingMessage: normalize(snapshot(agent.state.streamingMessage ?? null)),
		pendingToolCalls: [...agent.state.pendingToolCalls],
		errorMessage: agent.state.errorMessage ?? null,
	};
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined)
		throw new Error("agent oracle requires a source module path");
	const loopApi = await import(pathToFileURL(sourceModulePath).href);
	const agentApi = await import(
		new URL("./agent.ts", pathToFileURL(sourceModulePath)).href
	);
	const ai = await import("@earendil-works/pi-ai");
	const api = { ...loopApi, Agent: agentApi.Agent, ai };
	const readline = await import("node:readline");
	const lines = readline.createInterface({ input: process.stdin });
	for await (const line of lines) {
		if (!line.trim()) continue;
		let id: RequestId = null;
		try {
			const request = JSON.parse(line) as ScenarioRequest;
			id = request.id;
			const output = await executeScenario(api, request.scenario);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			const message = error instanceof Error ? error.message : String(error);
			process.stdout.write(
				`${JSON.stringify({ id, ok: false, error: { message } })}\n`,
			);
		}
	}
}

void main().catch((error) => {
	process.stderr.write(
		`${error instanceof Error ? error.stack : String(error)}\n`,
	);
	process.exit(1);
});
