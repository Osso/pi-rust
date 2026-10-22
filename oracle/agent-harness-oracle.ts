import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const root = resolve(dirname(source), "../../../..");
	const { AgentHarness } = await import(pathToFileURL(source).href);
	const { Session } = await import(
		pathToFileURL(resolve(dirname(source), "session/session.ts")).href
	);
	const { InMemorySessionStorage } = await import(
		pathToFileURL(resolve(dirname(source), "session/memory-storage.ts")).href
	);
	const { NodeExecutionEnv } = await import(
		pathToFileURL(resolve(dirname(source), "env/nodejs.ts")).href
	);
	const { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } =
		await import(pathToFileURL(resolve(root, "packages/ai/src/index.ts")).href);
	// Faux never uses HTTP; forbid accidental network access through the real global boundary.
	globalThis.fetch = async () => {
		throw new Error("unexpected oracle HTTP request");
	};
	const epoch = 1700000000000;
	Date.now = () => epoch;

	type ObjectValue = Record<string, any>;
	function clean(value: any): any {
		if (Array.isArray(value)) return value.map(clean);
		if (!value || typeof value !== "object") return value;
		return Object.fromEntries(
			Object.entries(value)
				.filter(([key]) => !["timestamp", "responseId"].includes(key))
				.map(([key, value]) => [key, clean(value)]),
		);
	}
	function entryValue(entry: ObjectValue): ObjectValue {
		const { id, parentId, timestamp, ...value } = entry;
		return clean(value);
	}
	function eventValue(event: ObjectValue): ObjectValue {
		if (event.type === "message_update") return { type: event.type };
		if (event.type === "queue_update")
			return {
				type: event.type,
				steer: event.steer.length,
				followUp: event.followUp.length,
				nextTurn: event.nextTurn.length,
			};
		if (event.type === "save_point")
			return {
				type: event.type,
				hadPendingMutations: event.hadPendingMutations,
			};
		if (event.type === "settled")
			return { type: event.type, nextTurnCount: event.nextTurnCount };
		return { type: event.type };
	}
	function failure(error: any): ObjectValue {
		return { code: error.code, message: error.message };
	}
	async function run(request: ObjectValue): Promise<ObjectValue> {
		const models = createModels();
		const faux = fauxProvider({
			api: "fixture-api",
			provider: "fixture",
			tokensPerSecond: 1000000000,
			tokenSize: { min: 100000, max: 100000 },
		});
		models.setProvider(faux.provider);
		const requests: any[] = [];
		faux.setResponses(
			Array.from(
				{ length: 8 },
				(_, index) => async (context: any, options: any) => {
					requests.push(
						clean({
							model: faux.getModel().id,
							systemPrompt: context.systemPrompt,
							messages: context.messages,
							options: {
								timeoutMs: options.timeoutMs,
								maxRetries: options.maxRetries,
								maxRetryDelayMs: options.maxRetryDelayMs,
								headers: options.headers,
								metadata: options.metadata,
								sessionId: options.sessionId,
								reasoning: options.reasoning,
								transport: options.transport,
								cacheRetention: options.cacheRetention,
							},
						}),
					);
					if (request.scenario.startsWith("payload"))
						requests.push({
							payloadResult: await options.onPayload(
								{ native: "\ud800" },
								faux.getModel(),
							),
						});
					if (request.scenario.startsWith("tool") && index === 0)
						return fauxAssistantMessage(
							fauxToolCall("lookup", { value: "raw" }, { id: "call" }),
						);
					return fauxAssistantMessage(`reply-${index + 1}`);
				},
			),
		);
		const entries = request.entries ?? [];
		const session = new Session(
			new InMemorySessionStorage({
				entries,
				metadata: { id: "session-1", createdAt: "now" },
			}),
		);
		const tool = (name: string) => ({
			name,
			label: name,
			description: "fixture",
			parameters: {
				type: "object",
				properties: { value: { type: "string" } },
				required: ["value"],
			},
			execute: async (_id: string, args: any) => ({
				content: [{ type: "text", text: args.value }],
				details: { native: "\ud800" },
			}),
		});
		const harness = new AgentHarness({
			models,
			session,
			env: new NodeExecutionEnv({ cwd: "/tmp" }),
			model: faux.getModel(),
			tools: request.scenario.startsWith("tool") ? [tool("lookup")] : [],
			streamOptions: request.streamOptions,
			steeringMode: request.mode ?? "one-at-a-time",
			followUpMode: request.mode ?? "one-at-a-time",
			resources: {
				skills: [
					{
						name: "skill",
						description: "desc",
						content: "body\ud800",
						filePath: "/skills/SKILL.md",
					},
				],
				promptTemplates: [{ name: "template", content: "$1 $@" }],
			},
		});
		const events: any[] = [];
		const results: any[] = [];
		let activated = false;
		let rejectDrain = false;
		harness.subscribe(async (event: any) => {
			events.push(eventValue(event));
			if (
				request.scenario === "writes" &&
				event.type === "turn_end" &&
				!activated
			) {
				activated = true;
				await harness.appendMessage({
					role: "user",
					content: "deferred\ud800",
					timestamp: epoch,
				});
				await harness.setModel({ ...faux.getModel(), name: "renamed" });
				await harness.setThinkingLevel("high");
				await harness.setActiveTools([]);
			}
			if (
				request.scenario === "queues" &&
				event.type === "message_start" &&
				event.message.role === "assistant" &&
				!activated
			) {
				activated = true;
				await harness.steer("steer-1");
				await harness.steer("steer-2");
				await harness.followUp("follow-1");
				await harness.followUp("follow-2");
			}
			if (
				request.scenario === "rollback" &&
				event.type === "queue_update" &&
				rejectDrain
			) {
				rejectDrain = false;
				throw new Error("queue\ud800");
			}
			if (
				request.scenario === "failure-report" &&
				event.type === "message_end" &&
				event.message.role === "assistant"
			)
				throw new Error("report\ud800");
			if (
				request.scenario === "turn-listener-error" &&
				event.type === "turn_end" &&
				!activated
			) {
				activated = true;
				await harness.setThinkingLevel("high");
				throw new Error("turn\ud800");
			}
		});
		if (["request-error", "failure-report"].includes(request.scenario))
			harness.on("before_provider_request", () => {
				throw new Error("request\ud800");
			});
		if (request.scenario === "start-error")
			harness.on("before_agent_start", () => {
				throw new Error("start\ud800");
			});
		if (request.scenario === "context") {
			harness.on("before_agent_start", () => ({
				messages: [
					{ role: "user", content: "injected\ud800", timestamp: epoch },
				],
				systemPrompt: "override\ud800",
			}));
			harness.on("context", (event: any) => ({
				messages: [
					...event.messages,
					{ role: "user", content: "context\udc00", timestamp: epoch },
				],
			}));
		}
		if (request.scenario === "patches") {
			harness.on("before_provider_request", () => ({
				streamOptions: {
					headers: { remove: undefined, one: "a\ud800" },
					metadata: { remove: undefined, one: "\udc00" },
					timeoutMs: undefined,
				},
			}));
			harness.on("before_provider_request", (event: any) => {
				results.push(clean(event.streamOptions));
				return {
					streamOptions: {
						headers: { two: "b" },
						metadata: { two: [null, "\ud800"] },
					},
				};
			});
		}
		if (request.scenario === "live-hooks") {
			const late = () => {
				results.push("late");
				return { streamOptions: { timeoutMs: 42 } };
			};
			harness.on("before_provider_request", () => {
				harness.on("before_provider_request", late);
				results.push("first");
			});
		}
		if (request.scenario.startsWith("payload")) {
			harness.on("before_provider_payload", () => {
				if (request.scenario === "payload-error")
					throw new Error("payload\ud800");
				return { payload: undefined };
			});
			harness.on("before_provider_payload", (event: any) => {
				results.push({ payloadStage: 2, present: event.payload !== undefined });
				return { payload: { finish: "\udc00" } };
			});
		}
		if (request.treeHook)
			harness.on("session_before_tree", () => request.treeHook);
		if (request.compactHook)
			harness.on("session_before_compact", () => request.compactHook);
		if (request.scenario.startsWith("tool")) {
			harness.on("tool_call", (event: any) => {
				results.push({ hook: "call", input: event.input });
				return request.scenario === "tool-block"
					? { block: true, reason: "blocked\ud800" }
					: undefined;
			});
			harness.on("tool_result", (event: any) => {
				results.push({
					hook: "result",
					input: event.input,
					content: event.content,
					details: event.details,
					isError: event.isError,
				});
				return request.scenario === "tool-terminate"
					? {
							content: [{ type: "text", text: "patched\udc00" }],
							details: null,
							isError: true,
							terminate: true,
						}
					: undefined;
			});
		}
		for (const operation of request.operations ?? [
			{ op: "prompt", text: "hello\ud800" },
		]) {
			try {
				let value: any;
				switch (operation.op) {
					case "next":
						await harness.nextTurn(operation.text);
						value = null;
						break;
					case "prompt":
						if (request.scenario === "rollback") rejectDrain = true;
						value = clean(await harness.prompt(operation.text));
						break;
					case "skill":
						value = clean(
							await harness.skill(operation.name, operation.instructions),
						);
						break;
					case "template":
						value = clean(
							await harness.promptFromTemplate(operation.name, operation.args),
						);
						break;
					case "steer":
						await harness.steer(operation.text);
						value = null;
						break;
					case "follow":
						await harness.followUp(operation.text);
						value = null;
						break;
					case "abort":
						value = clean(await harness.abort());
						break;
					case "compact":
						value = clean(await harness.compact(operation.instructions));
						break;
					case "navigate":
						value = clean(
							await harness.navigateTree(operation.id, operation.options),
						);
						if (value.summaryEntry)
							value.summaryEntry = entryValue(value.summaryEntry);
						break;
					case "active":
						await harness.setActiveTools(operation.names);
						value = null;
						break;
					case "tools":
						await harness.setTools(operation.names.map(tool), operation.active);
						value = null;
						break;
					case "settings":
						await harness.setModel({ ...faux.getModel(), name: "changed" });
						await harness.setThinkingLevel("low");
						await harness.setResources({
							promptTemplates: [{ name: "new", content: "native\udc00" }],
						});
						await harness.setSteeringMode("all");
						await harness.setFollowUpMode("all");
						value = {
							modelName: harness.getModel().name,
							tools: harness.getTools().map((tool: any) => tool.name),
							active: harness.getActiveTools().map((tool: any) => tool.name),
							resources: harness.getResources(),
							streamOptions: harness.getStreamOptions(),
							steering: harness.getSteeringMode(),
							followUp: harness.getFollowUpMode(),
						};
						break;
					default:
						throw new Error(`unsupported fixture operation: ${operation.op}`);
				}
				results.push({ ok: value });
			} catch (error) {
				results.push({ error: failure(error) });
			}
		}
		return {
			results,
			events,
			requests,
			entries: (await session.getEntries()).map(entryValue),
			thinking: harness.getThinkingLevel(),
		};
	}
	const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of input) {
		try {
			console.log(JSON.stringify(await run(JSON.parse(line))));
		} catch (error) {
			console.log(
				JSON.stringify({
					fixtureError: String(error),
					stack: (error as Error).stack,
				}),
			);
		}
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
