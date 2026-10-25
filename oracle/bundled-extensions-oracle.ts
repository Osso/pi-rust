import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import { rename } from "node:fs/promises";

type RequestId = string | number | null;

type Step = {
	op: string;
	name?: string;
	args?: string;
	prefix?: string;
	event?: Record<string, unknown>;
	toolName?: string;
	params?: Record<string, unknown>;
	mode?: string;
	hasUI?: boolean;
	idle?: boolean;
	confirmed?: boolean;
	activeAgents?: Array<Record<string, unknown>>;
	inputs?: Array<string | null>;
	runCommandResults?: Array<{ stdout: string; stderr: string; exitCode: number | null }>;
	env?: Record<string, string>;
	sessionFile?: string | null;
	sessionId?: string;
	entries?: Array<Record<string, unknown>>;
	controlDbPath?: string | null;
	cwd?: string;
	renameError?: string;
	aborted?: boolean;
};

type Request = {
	id: RequestId;
	extension: string;
	steps: Step[];
	useDefaultProvision?: boolean;
};

type Calls = {
	shutdown: number;
	confirms: Array<[string,string]>;
	notify: Array<[string, string | null]>;
	setStatus: Array<[string, string | null]>;
	setEditorText: string[];
	sendUserMessage: Array<[string, Record<string, unknown> | undefined]>;
	appendEntry: Array<[string, unknown]>;
	restart: Array<Record<string, unknown>>;
	sandboxProfile: Array<[string | null, string]>;
	showSandboxSelector: number;
	showApprovalSelector: number;
	inputs: Array<[string, string | null, Record<string, unknown> | undefined]>;
	provision: Array<[Record<string, unknown>, string[]]>;
	runCommand: Array<[string, string[], boolean]>;
	debug: Array<[string, string | null]>;
	gate: unknown[];
	errors: Array<{ name: string; message: string }>;
};

function newCalls(): Calls {
	return {
        shutdown: 0,
        confirms: [],
		notify: [],
		setStatus: [],
		setEditorText: [],
		sendUserMessage: [],
		appendEntry: [],
		restart: [],
		sandboxProfile: [],
		showSandboxSelector: 0,
		showApprovalSelector: 0,
		inputs: [],
		provision: [],
		runCommand: [],
		debug: [],
		gate: [],
		errors: [],
	};
}

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function createHarness(module: Record<string, unknown>, extension: string, callsRef: { calls: Calls }, useDefaultProvision = false) {
	const handlers = new Map<string, Array<(event: unknown, ctx: unknown) => unknown>>();
	const tools = new Map<string, Record<string, unknown>>();
	const commands = new Map<string, Record<string, unknown>>();
	const gates: Array<(event: unknown, ctx: unknown) => unknown> = [];
	const flags = new Map<string, Record<string, unknown>>();
	const shortcuts = new Map<string, Record<string, unknown>>();
	const handlerOrder: string[] = [];
	const state: { inputs: Array<string | null>; runCommandResults: Step["runCommandResults"]; renameError?: string } = {
		inputs: [],
		runCommandResults: [],
	};

	const provision = async (
		request: Record<string, unknown>,
		values: string[],
	): Promise<void> => {
		callsRef.calls.provision.push([request, [...values]]);
	};

	const runCommand = async (
		executable: string,
		args: string[],
		_session: string,
		signal?: AbortSignal,
	): Promise<{ stdout: string; stderr: string; exitCode: number | null }> => {
		callsRef.calls.runCommand.push([executable, args, signal !== undefined]);
		const next = state.runCommandResults?.shift();
		if (!next) throw new Error("no fake browser-cli result queued");
		return next;
	};

	const pi = {
		on(event: string, handler: (event: unknown, ctx: unknown) => unknown) {
			handlerOrder.push(event);
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		registerTool(tool: Record<string, unknown>) {
			tools.set(tool.name as string, tool);
		},
		registerCommand(name: string, options: Record<string, unknown>) {
			commands.set(name, options);
		},
		registerFlag(name: string, options: Record<string, unknown>) {
			flags.set(name, options);
		},
		registerShortcut(key: string, options: Record<string, unknown>) {
			shortcuts.set(key, options);
		},
		registerToolGate(gate: (event: unknown, ctx: unknown) => unknown) {
			gates.push(gate);
		},
		appendEntry(customType: string, data: unknown) {
			callsRef.calls.appendEntry.push([customType, data]);
		},
		sendUserMessage(message: string, options?: Record<string, unknown>) {
			callsRef.calls.sendUserMessage.push([message, options]);
		},
		sendMessage() {},
		exec() {
			throw new Error("exec is not used by these extensions");
		},
	};

	const factory = module.default as (api: unknown, ...rest: unknown[]) => void;
	if (extension === "debug") {
		factory(pi, () => ({
			enable: async (sessionId: string) => {
				callsRef.calls.debug.push(["enable", sessionId]);
				return `debug ${sessionId}`;
			},
			disable: async () => {
				callsRef.calls.debug.push(["disable", null]);
			},
		}));
	} else {
		factory(pi);
	}

	if (typeof module.createAskSecretToolDefinition === "function") {
		const create = module.createAskSecretToolDefinition as (options: unknown) => Record<string, unknown>;
		const renameFile = async (source: string, destination: string) => {
			if (state.renameError) throw new Error(state.renameError);
			await rename(source, destination);
		};
		const tool = create({ ...(useDefaultProvision ? {} : { provision }), renameFile });
		tools.set(tool.name as string, tool);
	} else if (typeof module.createBrowserCliToolDefinition === "function") {
		const create = module.createBrowserCliToolDefinition as (options: unknown) => Record<string, unknown>;
		const tool = create({ runCommand });
		tools.set(tool.name as string, tool);
	}

	const makeCtx = (step: Step) => {
		state.inputs = [...(step.inputs ?? [])];
		state.renameError = step.renameError;
		state.runCommandResults = [...(step.runCommandResults ?? [])];
		const ui = {
			confirm: async (title: string, message: string) => {
                callsRef.calls.confirms.push([title, message]);
                return step.confirmed ?? false;
            },
			input: async (title: string, placeholder: string | null, options?: Record<string, unknown>) => {
				callsRef.calls.inputs.push([title, placeholder, options]);
				if (state.inputs.length === 0) throw new Error("no fake input queued");
				const value = state.inputs.shift();
				return value === null ? undefined : value;
			},
			notify: (message: string, type?: string) => {
				callsRef.calls.notify.push([message, type ?? null]);
			},
			setStatus: (key: string, text?: string) => {
				callsRef.calls.setStatus.push([key, text ?? null]);
			},
			setEditorText: (text: string) => {
				callsRef.calls.setEditorText.push(text);
			},
		};

		return {
			mode: step.mode ?? "tui",
			hasUI: step.hasUI ?? true,
			cwd: step.cwd ?? process.cwd(),
			ui,
			isIdle: () => step.idle ?? true,
			signal: undefined,
			sessionManager: {
				getEntries: () => step.entries ?? [],
				getSessionFile: () => step.sessionFile ?? undefined,
				getSessionId: () => step.sessionId ?? "test-session-id",
			},
			shutdown: () => {
                callsRef.calls.shutdown++;
            },
			multiAgentStore: { listActiveAgents: () => step.activeAgents ?? [] },
			controlDbPath: step.controlDbPath ?? undefined,
			restart: async (options: Record<string, unknown>) => {
				callsRef.calls.restart.push(options);
			},
			setSandboxProfile: (profile: string | undefined, scope: string) => {
				callsRef.calls.sandboxProfile.push([profile ?? null, scope]);
			},
			showSandboxSelector: () => {
				callsRef.calls.showSandboxSelector += 1;
			},
			showApprovalSelector: () => {
				callsRef.calls.showApprovalSelector += 1;
			},
		};
	};

	return { handlers, tools, commands, gates, flags, shortcuts, handlerOrder, makeCtx };
}

function registrations(h: ReturnType<typeof createHarness>) {
	return {
		tools: [...h.tools.values()].map((tool) => ({
			name: tool.name,
			label: tool.label,
			description: tool.description,
			promptSnippet: tool.promptSnippet ?? null,
			promptGuidelines: tool.promptGuidelines ?? null,
			approvalRequired: tool.approvalRequired ?? null,
			parameters: tool.parameters,
		})),
		commands: [...h.commands.entries()].map(([name, command]) => ({
			name,
			description: command.description ?? null,
			hasCompletions: typeof command.getArgumentCompletions === "function",
		})),
		flags: [...h.flags.entries()].map(([name, flag]) => ({ name, flag })),
		shortcuts: [...h.shortcuts.keys()],
		handlers: h.handlerOrder,
		toolGates: h.gates.length,
	};
}

async function executeStep(
	h: ReturnType<typeof createHarness>,
	step: Step,
): Promise<unknown> {
	for (const [name, value] of Object.entries(step.env ?? {})) {
		process.env[name] = value;
	}
	const ctx = h.makeCtx(step);
	switch (step.op) {
		case "registrations":
			return registrations(h);
		case "command": {
			const command = h.commands.get(step.name as string);
			if (!command) throw new Error(`command not registered: ${step.name}`);
			await (command.handler as (args: string, ctx: unknown) => unknown)(step.args ?? "", ctx);
			return null;
		}
		case "completions": {
			const command = h.commands.get(step.name as string);
			if (!command) throw new Error(`command not registered: ${step.name}`);
			const get = command.getArgumentCompletions as ((prefix: string) => unknown) | undefined;
			if (!get) return null;
			const cwd = step.cwd ?? process.cwd();
			const previous = process.cwd();
			process.chdir(cwd);
			try {
				return get(step.prefix ?? "") ?? null;
			} finally {
				process.chdir(previous);
			}
		}
		case "env": {
			return {
				PLAN_FILE: process.env.PLAN_FILE ?? null,
				PLAN_PATH: process.env.PLAN_PATH ?? null,
				PI_PLAN_FILE: process.env.PI_PLAN_FILE ?? null,
				PI_PLAN_PATH: process.env.PI_PLAN_PATH ?? null,
			};
		}
        case "archiveShutdownStderr": {
            const previousWrite = process.stderr.write;
            let output = "";
            process.stderr.write = ((chunk: string | Uint8Array) => {
                output += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString();
                return true;
            }) as typeof process.stderr.write;
            try {
                for (const handler of h.handlers.get("session_shutdown") ?? []) {
                    await handler({ type: "session_shutdown", reason: "quit" }, ctx);
                }
                return output;
            } finally {
                process.stderr.write = previousWrite;
            }
        }
		case "event": {			const handlers = h.handlers.get(step.name as string) ?? [];
			let result: unknown;
			for (const handler of handlers) {
				result = await handler({ type: step.name, ...(step.event ?? {}) }, ctx);
			}
			return result ?? null;
		}
		case "gate": {
			let result: unknown;
			for (const gate of h.gates) {
				result = await gate(
					{
						type: "tool_call",
						toolCallId: `${step.toolName}-call`,
						toolName: step.toolName,
						input: {},
						bypassPermissions: false,
					},
					ctx,
				);
			}
			return result ?? null;
		}
		case "tool": {
			const tool = h.tools.get(step.name as string);
			if (!tool) throw new Error(`tool not registered: ${step.name}`);
			const execute = tool.execute as (
				id: string,
				params: unknown,
				signal: unknown,
				onUpdate: unknown,
				ctx: unknown,
			) => unknown;
			return await execute("call-1", step.params ?? {}, step.aborted ? AbortSignal.abort() : undefined, undefined, ctx);
		}
		default:
			throw new Error(`unknown step: ${step.op}`);
	}
}

async function execute(request: Request, module: Record<string, unknown>) {
	const results: unknown[] = [];
	const callsRef = { calls: newCalls() };
	const h = createHarness(module, request.extension, callsRef, request.useDefaultProvision);
	for (const step of request.steps) {
		callsRef.calls = newCalls();
		let result: unknown = null;
		let error: { name: string; message: string } | null = null;
		try {
			result = await executeStep(h, step);
		} catch (caught) {
			error = normalizeError(caught);
		}
		results.push({ op: step.op, result, error, calls: callsRef.calls });
	}
	return results;
}

function executeRequestLine(line: string, module: Record<string, unknown>): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		void execute(request, module).then(
			(results) => {
				process.stdout.write(`${JSON.stringify({ id, ok: true, output: { steps: results } })}\n`);
			},
			(error: unknown) => {
				process.stdout.write(
					`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`,
				);
			},
		);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(module: Record<string, unknown>): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), module);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) {
		throw new Error("bundled extensions oracle requires a source module path");
	}
	const module = (await import(pathToFileURL(modulePath).href)) as Record<string, unknown>;
	serveRequests(module);
}

void main().catch((error: unknown) => {
	process.stderr.write(`bundled extensions oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
