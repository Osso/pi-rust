// Differential oracle for coding-agent core leaf modules.
// argv[2] is the pinned `packages/coding-agent/src` directory; each request names
// the module (relative to it) and an operation handled below.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Api = Record<string, (...args: unknown[]) => unknown> & Record<string, unknown>;
type Request = { id: number; module: string; operation: string; cases?: unknown[]; [key: string]: unknown };
type Handler = (api: Api, request: Request) => unknown | Promise<unknown>;

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
}

function cases(request: Request): unknown[] {
	if (!Array.isArray(request.cases)) throw new Error(`${request.operation} requires cases`);
	return request.cases;
}

const handlers: Record<string, Handler> = {
	policy: (api, request) =>
		cases(request).map((value) => ({
			isApprovalPolicy: api.isApprovalPolicy(value),
			normalized: api.normalizeApprovalPolicy(value),
		})),
	policyEvaluate: (api) =>
		["on-request", "never", "auto-approve"].flatMap((policy) =>
			[true, false].map((approvalRequired) => api.evaluateApprovalPolicy(policy, { approvalRequired })),
		),
	presets: (api, request) => ({
		approvalPresets: api.APPROVAL_PRESETS,
		sandboxProfiles: api.SANDBOX_PROFILES,
		found: (api.APPROVAL_PRESETS as Array<{ name: string }>).map((preset) => api.findApprovalPreset(preset.name)),
		forPolicy: ["on-request", "never", "auto-approve"].map((policy) => api.approvalPresetForPolicy(policy)),
		bypass: (api.APPROVAL_PRESETS as Array<{ name: string }>).map((preset) =>
			api.approvalPresetToBypassPermissions(preset.name),
		),
		names: cases(request).map((value) => ({
			preset: api.isApprovalPresetName(value),
			sandbox: api.isSandboxProfileName(value),
		})),
	}),
	providerDisplayNames: (api) => Object.entries(api.BUILT_IN_PROVIDER_DISPLAY_NAMES as Record<string, string>),
	radius: (api) => api.RADIUS_PROVIDER_ID,
	toolCapabilities: (api) => ({
		child: api.CHILD_DISABLED_AGENT_TOOL_NAMES,
		supervisor: api.SUPERVISOR_ONLY_TOOL_NAMES,
	}),
	sessionErrors: (api, request) =>
		cases(request).map((filePath) => {
			const ErrorClass = api.SessionImportFileNotFoundError as unknown as new (path: string) => Error & {
				filePath: string;
			};
			const error = new ErrorClass(filePath as string);
			return { name: error.name, message: error.message, filePath: error.filePath };
		}),
	piManifest: (api, request) => cases(request).map((path) => api.readPiManifest(path)),
	auth: (api, request) => cases(request).map((options) => api.parseAuthInput(options)),
	httpDispatcher: (api, request) => ({
		parsed: cases(request).map((value) => {
			const parsed = api.parseHttpIdleTimeoutMs(value);
			return parsed === undefined ? null : parsed;
		}),
		formatted: (request.timeouts as number[]).map((value) => api.formatHttpIdleTimeoutMs(value)),
	}),
	runtimeProcess: (api, request) =>
		cases(request).map((value) => api.commandLineIsPiRuntime(value)),
	transportAddress: (api, request) =>
		cases(request).map((value) => {
			const input = value as { value: string; option: "--listen" | "--connect" };
			const result = api.parseTransportAddress(input.value, input.option) as {
				address?: unknown;
				error?: string;
			};
			return result.address === undefined ? { ok: false, error: result.error } : { ok: true, address: result.address };
		}),
	command: (api, request) => {
		type Invocation = { command: string; name: string | null; count: number | null; remaining: string[] };
		type Option<T> = { name: string; parse(value: string): { ok: true; value: T } | { ok: false; error: string } };
		const stringOption = api.stringOption as (name: string) => Option<string>;
		const valueOption = api.valueOption as <T>(
			name: string,
			parse: (value: string) => { ok: true; value: T } | { ok: false; error: string },
		) => Option<T>;
		type CommandLike = {
			option(option: Option<unknown>): CommandLike;
			build(builder: (input: unknown) => unknown): CommandLike;
			action(action: (command: unknown, context: unknown) => void): CommandLike;
			command(subcommand: CommandLike, ...rest: unknown[]): CommandLike;
			parse(argv: string[]): { ok: boolean; command?: Invocation; errors?: string[] };
			execute(argv: string[], context: unknown): Promise<{ ok: boolean; command?: Invocation; errors?: string[] }>;
		};
		const Command = api.Command as unknown as new (name: string) => CommandLike;
		const name = stringOption("--name");
		const count = valueOption<number>("--count", (value) => {
			const parsed = Number(value);
			return Number.isInteger(parsed)
				? { ok: true, value: parsed }
				: { ok: false, error: `--count must be a number, got "${value}"` };
		});
		const buildInvocation =
			(commandName: string) =>
			(input: any) => ({
				ok: true as const,
				command: {
					command: commandName,
					name: input.value(name) ?? null,
					count: input.value(count) ?? null,
					remaining: [...input.remainingArgs],
				} satisfies Invocation,
			});
		const root = new Command("pi");
		root.option(name);
		root.option(count);
		root.build(buildInvocation("pi"));
		root.action((_command, context) => (context as string[]).push("pi"));
		const child = new Command("server");
		const listen = stringOption("--listen");
		child.option(listen);
		child.build((input: any) => ({
			ok: true as const,
			command: {
				command: "server",
				name: input.value(listen) ?? null,
				count: null,
				remaining: [...input.remainingArgs],
			} satisfies Invocation,
		}));
		child.action((_command, context) => (context as string[]).push("server"));
		root.command(child);
		return (request.argvCases as string[][]).map((argv) => {
			const result = root.parse(argv);
			return result.ok ? { ok: true, command: result.command } : { ok: false, errors: result.errors };
		});
	},
	projectResolver: (api, request) => ({
		resolved: (request.resolveCases as Array<{ configPath: string; cwd: string; remotes: string[] }>).map((entry) =>
			api.resolveSupervisorProject({
				config: api.readSupervisorProjectConfig(entry.configPath),
				cwd: entry.cwd,
				remoteRepositoryNames: entry.remotes,
			}),
		),
		identities: (request.urls as string[]).map((url) => ({
			identity: api.extractRemoteRepositoryIdentity(url) ?? null,
			name: api.extractRemoteRepositoryName(url) ?? null,
		})),
		configs: (request.configPaths as string[]).map((path) => {
			try {
				return { ok: true, config: api.readSupervisorProjectConfig(path) };
			} catch (error) {
				return { ok: false, error: errorOf(error).message };
			}
		}),
	}),
	sessionHealth: (api, request) => {
		const nowIso = request.nowIso as string;
		const base = { ...(api.emptySessionHealth("session-a", nowIso) as Record<string, unknown>) };
		const live = { ...base, agentGeneration: 1, pid: 1234, checkStatus: "ok", checkedGeneration: 1 };
		return {
			empty: base,
			sticky: [
				{ ...base, agentGeneration: 2, checkStatus: "dead", checkedGeneration: 2 },
				{ ...base, agentGeneration: 3, checkStatus: "dead", checkedGeneration: 2 },
			].map((health) => api.isStickyDead(health)),
			eligible: [live, { ...live, checkStatus: "never" }, { ...live, checkStatus: "dead" }].map((health) =>
				api.isSessionEligibleToReceive(health),
			),
			ended: api.endSessionHealth(live, nowIso),
			generations: (() => {
				const first = api.advanceGenerationForPid(base, 42, nowIso);
				const same = api.advanceGenerationForPid(first, 42, nowIso);
				const changed = api.advanceGenerationForPid(same, 43, nowIso);
				const zero = api.advanceGenerationForPid({ ...base, pid: 7 }, 7, nowIso);
				return [first, same, changed, zero];
			})(),
			goals: cases(request).map((value) => api.parseGoalObjective(value)),
			liveStatus: (request.liveCases as Array<{ health: Record<string, unknown>; lastActiveAt: string | null }>).map(
				(entry) =>
					api.deriveLiveStatus(
						{ ...live, ...entry.health },
						entry.lastActiveAt,
						request.nowMs as number,
						request.staleMs as number,
					),
			),
		};
	},
};

type ResidentServerState = {
	server: any;
	prompts: Array<{ id: string; text: string }>;
	listeners: Set<(event: unknown) => void>;
	unsubscribers: number;
};

let residentServer: ResidentServerState | undefined;

const residentHandlers: Record<string, Handler> = {
	residentConsoleServer: async (api, request) => {
		const options = request as unknown as {
			socketPath: string;
			snapshot: unknown;
			rejectPrompt?: string;
		};
		const state: ResidentServerState = {
			server: undefined,
			prompts: [],
			listeners: new Set(),
			unsubscribers: 0,
		};
		const Server = api.ResidentConsoleServer as unknown as new (options: unknown) => any;
		const server = new Server({
			socketPath: options.socketPath,
			service: "supervisor",
			getSnapshot: () => options.snapshot,
			enqueuePrompt: async (text: string, id: string) => {
				state.prompts.push({ id, text });
				if (options.rejectPrompt) throw new Error(options.rejectPrompt);
			},
			subscribe: (listener: (event: unknown) => void) => {
				state.listeners.add(listener);
				return () => {
					state.unsubscribers += 1;
					state.listeners.delete(listener);
				};
			},
		});
		await server.start();
		state.server = server;
		residentServer = state;
		return { started: true, socketPath: options.socketPath };
	},
	residentConsoleReceived: () => ({
		prompts: residentServer?.prompts ?? [],
		listeners: residentServer?.listeners.size ?? 0,
		unsubscribers: residentServer?.unsubscribers ?? 0,
	}),
	residentConsolePublish: (api, request) => {
		const event = (request as unknown as { event: unknown }).event;
		for (const listener of residentServer?.listeners ?? []) listener(event);
		return { published: residentServer?.listeners.size ?? 0 };
	},
	residentConsoleStop: async () => {
		await residentServer?.server?.close();
		residentServer = undefined;
		return { stopped: true };
	},
	residentConsoleClient: async (api, request) => {
		const input = request as unknown as {
			socketPath: string;
			prompt?: { id: string; text: string };
			eventCount: number;
		};
		const Client = api.ResidentConsoleClient as unknown as {
			connect(options: { socketPath: string; service: string }): Promise<any>;
		};
		const client = await Client.connect({ socketPath: input.socketPath, service: "supervisor" });
		const events: unknown[] = [];
		client.onEvent((event: unknown) => events.push(event));
		let promptError: string | null = null;
		if (input.prompt) {
			try {
				await client.prompt(input.prompt.id, input.prompt.text);
			} catch (error) {
				promptError = errorOf(error).message;
			}
		}
		const deadline = Date.now() + 3000;
		while (events.length < input.eventCount && Date.now() < deadline) {
			await new Promise((resolve) => setTimeout(resolve, 5));
		}
		const snapshot = client.snapshot;
		await client.close();
		return { snapshot, events, promptError };
	},
};

async function main(): Promise<void> {
	const sourceRoot = process.argv[2];
	if (!sourceRoot) throw new Error("core leaves oracle requires the coding-agent src directory");
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
			const handler = handlers[request.operation] ?? residentHandlers[request.operation];
			if (!handler) throw new Error(`unknown operation ${request.operation}`);
			const output = await handler(await load(request.module), request);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorOf(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`core leaves oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});

