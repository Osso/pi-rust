/**
 * Deterministic mode harness used by the print/RPC differential tests.
 *
 * argv[2] is the pinned `rpc-mode.ts` or `print-mode.ts` module path. Environment
 * configuration mirrors `crates/pi-coding-agent/tests/bin/pi_mode_harness.rs`.
 */
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";

type Json = Record<string, unknown>;

function requiredEnv(name: string): string {
	const value = process.env[name];
	if (value === undefined) throw new Error(`${name} must be set`);
	return value;
}

function harnessTools(): string[] {
	return [];
}

function endTurnTool(ai: any): unknown {
	return {
		name: "end_turn",
		label: undefined,
		description: "End the current model turn. Provide a concise reason explaining why the turn is finished.",
		promptSnippet: "End the current model turn with a required reason",
		promptGuidelines: [
			"Call end_turn only when the task is complete, progress requires user input, or the user explicitly asks you to stop. If work remains and progress is possible, continue working instead of calling end_turn. Assistant text alone does not finish the turn.",
			"Provide one concise, non-empty reason.",
		],
		parameters: {
			type: "object",
			properties: {
				reason: {
					type: "string",
					description: "Required non-empty reason for ending the current turn.",
				},
			},
			required: ["reason"],
		},
		executionMode: undefined,
		approvalRequired: process.env.PI_HARNESS_APPROVAL === "1",
		execute: async (_toolCallId: string, params: { reason: string }) => {
			if (params.reason.trim() === "") throw new Error("end_turn reason must be a non-empty string");
			return {
				content: [{ type: "text", text: `Turn ended: ${params.reason}` }],
				details: { reason: params.reason },
				terminate: true,
			};
		},
	};
}

function optionalEnv(name: string): string | undefined {
	const value = process.env[name];
	return value === undefined || value === "" ? undefined : value;
}

async function main(): Promise<void> {
	const modeModulePath = process.argv[2];
	if (modeModulePath === undefined) throw new Error("mode harness requires a source module path");

	let packagesRoot = dirname(modeModulePath);
	while (resolve(packagesRoot, "..") !== "/" && !packagesRoot.endsWith("/packages")) {
		packagesRoot = resolve(packagesRoot, "..");
	}
	const aiRoot = resolve(packagesRoot, "ai/src");
	const codingAgentRoot = resolve(packagesRoot, "coding-agent/src");

	const ai = await import(pathToFileURL(resolve(aiRoot, "index.ts")).href);
	const sdk = await import(pathToFileURL(resolve(codingAgentRoot, "core/sdk.ts")).href);
	const servicesModule = await import(pathToFileURL(resolve(codingAgentRoot, "core/agent-session-services.ts")).href);
	const runtimeModule = await import(pathToFileURL(resolve(codingAgentRoot, "core/agent-session-runtime.ts")).href);
	const sessionManagerModule = await import(pathToFileURL(resolve(codingAgentRoot, "core/session-manager.ts")).href);
	const extensionRuntimeModule = await import(
		pathToFileURL(resolve(codingAgentRoot, "core/extensions/extension-runtime.ts")).href
	);
	const authStorageModule = await import(pathToFileURL(resolve(codingAgentRoot, "core/auth-storage.ts")).href);
	const modelRuntimeModule = await import(pathToFileURL(resolve(codingAgentRoot, "core/model-runtime.ts")).href);

	const mode = requiredEnv("PI_HARNESS_MODE");
	const cwd = requiredEnv("PI_HARNESS_CWD");
	const agentDir = requiredEnv("PI_HARNESS_AGENT_DIR");
	const sessionDir = requiredEnv("PI_HARNESS_SESSION_DIR");
	const responses = JSON.parse(requiredEnv("PI_HARNESS_RESPONSES")) as Json[];
	const prompts = JSON.parse(optionalEnv("PI_HARNESS_PROMPTS") ?? "[]") as string[];

	const faux = ai.fauxProvider({ api: "faux", tokenSize: { min: 1, max: 1 } });
	faux.setResponses(
		responses.map((response) => {
			if (response.tool !== undefined) {
				const tool = response.tool as Json;
				return ai.fauxAssistantMessage(
					ai.fauxToolCall(tool.name as string, tool.arguments ?? {}, { id: tool.id as string }),
				);
			}
			const blocks = [];
			if (typeof response.text === "string" && response.text.length > 0) blocks.push(ai.fauxText(response.text));
			if (typeof response.endTurn === "string") {
				blocks.push(ai.fauxToolCall("end_turn", { reason: response.endTurn }, { id: response.toolId as string }));
			}
			if (blocks.length > 0) return ai.fauxAssistantMessage(blocks);
			return ai.fauxAssistantMessage("", {
				stopReason: response.stopReason as string | undefined,
				errorMessage: response.errorMessage as string | undefined,
			});
		}),
	);

	const credentials = authStorageModule.AuthStorage.inMemory(
		process.env.PI_HARNESS_NO_AUTH === "1" ? {} : { faux: { type: "api_key", key: "faux-key" } },
	);
	const modelRuntime = await modelRuntimeModule.ModelRuntime.create({
		credentials,
		modelsPath: null,
		allowModelNetwork: false,
	});
	modelRuntime.registerNativeProvider(faux.provider);

	if (process.env.PI_HARNESS_DEBUG) {
		const probe = modelRuntime.stream(faux.getModel(), { messages: [{ role: "user", content: "probe", timestamp: 0 }] }, {});
		for await (const event of probe) {
			if (event.type === "done" || event.type === "error") break;
		}
		process.stderr.write(`debug: probe pending=${faux.getPendingResponseCount()} calls=${faux.state.callCount}\n`);
	}

	if (process.env.PI_HARNESS_DEBUG) {
		const originalSimple = faux.provider.streamSimple;
		faux.provider.streamSimple = ((m: unknown, c: unknown, o: unknown) => {
			process.stderr.write(`debug: faux.streamSimple called pending=${faux.getPendingResponseCount()}\n`);
			return originalSimple(m as never, c as never, o as never);
		}) as typeof faux.provider.streamSimple;
		const originalStream = faux.provider.stream;
		faux.provider.stream = ((m: unknown, c: unknown, o: unknown) => {
			process.stderr.write(`debug: faux.stream called pending=${faux.getPendingResponseCount()}\n`);
			return originalStream(m as never, c as never, o as never);
		}) as typeof faux.provider.stream;
	}

	const resourceLoader = {
		getExtensions: () => ({ extensions: [], errors: [], runtime: extensionRuntimeModule.createExtensionRuntime() }),
		getSkills: () => ({ skills: [], diagnostics: [] }),
		getPrompts: () => ({ prompts: [], diagnostics: [] }),
		getThemes: () => ({ themes: [], diagnostics: [] }),
		getAgentsFiles: () => ({ agentsFiles: [] }),
		getRulesFiles: () => ({ rulesFiles: [] }),
		getRulesContent: () => undefined,
		getSystemPrompt: () => undefined,
		getSystemPromptSource: () => undefined,
		getAppendSystemPrompt: () => [],
		getAppendSystemPromptSources: () => [],
		extendResources: () => {},
		reload: async () => {},
	};
	const services = await servicesModule.createAgentSessionServices({
		cwd,
		agentDir,
		modelRuntime,
		resourceLoaderOptions: undefined,
	});
	services.resourceLoader = resourceLoader;
	const sessionManager = sessionManagerModule.SessionManager.create(cwd, sessionDir);
	const created = await servicesModule.createAgentSessionFromServices({
		services,
		sessionManager,
		model: faux.getModel(),
		thinkingLevel: "off",
		tools: harnessTools(),
		customTools: [endTurnTool(ai) as never],
	});

	const createRuntime = async (options: {
		cwd: string;
		agentDir: string;
		sessionManager: unknown;
	}): Promise<unknown> => {
		const nextServices = await servicesModule.createAgentSessionServices({
			cwd: options.cwd,
			agentDir: options.agentDir,
			modelRuntime,
		});
		const nextSession = await servicesModule.createAgentSessionFromServices({
			services: nextServices,
			sessionManager: options.sessionManager,
			model: faux.getModel(),
			thinkingLevel: "off",
			tools: harnessTools(),
			customTools: [endTurnTool(ai) as never],
		});
		return { ...nextSession, services: nextServices, diagnostics: [] };
	};

	const runtime = new runtimeModule.AgentSessionRuntime(created.session, services, createRuntime);
	if (process.env.PI_HARNESS_DEBUG) {
		process.stderr.write(
			`debug: messages=${created.session.messages.length} calls=${faux.state.callCount} pending=${faux.getPendingResponseCount()} model=${created.session.model?.provider}/${created.session.model?.id} api=${created.session.model?.api} sameRuntime=${created.session.modelRuntime === modelRuntime} prompt=${JSON.stringify(created.session.agent.state.systemPrompt)}\n`,
		);
		const direct = modelRuntime.streamSimple(faux.getModel(), { messages: [{ role: "user", content: "direct", timestamp: 0 }] }, {});
		for await (const event of direct) {
			if (event.type === "done" || event.type === "error") break;
		}
		process.stderr.write(`debug: direct pending=${faux.getPendingResponseCount()}\n`);
	}

	if (mode === "rpc") {
		const rpcMode = await import(pathToFileURL(modeModulePath).href);
		await rpcMode.runRpcMode(runtime);
		return;
	}

	const printMode = await import(pathToFileURL(modeModulePath).href);
	if (process.env.PI_HARNESS_DEBUG) {
		process.stderr.write(`debug: before print calls=${faux.state.callCount}\n`);
	}
	const exitCode = await printMode.runPrintMode(runtime, {
		mode: mode === "print-json" ? "json" : "text",
		messages: prompts.slice(1),
		initialMessage: prompts[0],
	});
	process.exit(exitCode);
}

void main().catch((error: unknown) => {
	process.stderr.write(`mode harness failed: ${error instanceof Error ? error.stack : String(error)}\n`);
	process.exitCode = 1;
});
