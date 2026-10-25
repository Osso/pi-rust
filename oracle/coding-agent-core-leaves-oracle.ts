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
	presetNameUnits: (api, request) => (request.cases as number[][]).map((units) => {
		const value = String.fromCharCode(...units);
		return { preset: api.isApprovalPresetName(value), sandbox: api.isSandboxProfileName(value) };
	}),
	presets: (api, request) => ({
		approvalPresets: api.APPROVAL_PRESETS,
		sandboxProfiles: api.SANDBOX_PROFILES,
		supportedPlatforms: ["linux", "darwin", "win32", "freebsd", "aix", "openbsd", "sunos", "android"].map((platform) => api.isSandboxPlatformSupported(platform)),
		defaultPlatformSupported: api.isSandboxPlatformSupported(),
		found: (api.APPROVAL_PRESETS as unknown as Array<{ name: string }>).map(
			(preset) => api.findApprovalPreset(preset.name),
		),
		forPolicy: ["on-request", "never", "auto-approve"].map((policy) => api.approvalPresetForPolicy(policy)),
		bypass: (api.APPROVAL_PRESETS as unknown as Array<{ name: string }>).map(
			(preset) => api.approvalPresetToBypassPermissions(preset.name),
		),
		names: cases(request).map((value) => ({
			preset: api.isApprovalPresetName(value),
			sandbox: api.isSandboxProfileName(value),
		})),
	}),
	providerDisplayNames: (api) =>
		Object.entries(
			api.BUILT_IN_PROVIDER_DISPLAY_NAMES as unknown as Record<string, string>,
		),
	toolCapabilities: (api, request) => {
		const capabilities = api as unknown as {
			CHILD_DISABLED_AGENT_TOOL_NAMES: readonly string[];
			SUPERVISOR_ONLY_TOOL_NAMES: readonly string[];
			STANDALONE_DISABLED_TOOL_NAMES: ReadonlySet<string>;
		};
		return {
			child: capabilities.CHILD_DISABLED_AGENT_TOOL_NAMES,
			supervisor: capabilities.SUPERVISOR_ONLY_TOOL_NAMES,
			standalone: [...capabilities.STANDALONE_DISABLED_TOOL_NAMES],
			queries: cases(request).map((value) => {
				if (typeof value !== "string")
					throw new Error("tool capability query must be a string");
				return {
					name: value,
					disabled: capabilities.STANDALONE_DISABLED_TOOL_NAMES.has(value),
				};
			}),
		};
	},
	sessionErrors: (api, request) =>
		cases(request).map((filePath) => {
			const ErrorClass = api.SessionImportFileNotFoundError as unknown as new (path: string) => Error & {
				filePath: string;
			};
			const error = new ErrorClass(filePath as string);
			return { name: error.name, message: error.message, filePath: error.filePath };
		}),
	manifestDiscovery: async (api, request) => {
		const discover = api.discoverAndLoadExtensions as (
			paths: string[], cwd: string, agentDir: string,
		) => Promise<{ extensions: Array<{ path: string }>; errors: unknown[] }>;
		const results: string[][] = [];
		for (const directory of cases(request) as string[]) {
			const loaded = await discover([directory], request.cwd as string, request.agentDir as string);
			if (loaded.errors.length) throw new Error(JSON.stringify(loaded.errors));
			results.push(loaded.extensions.map(extension => extension.path));
		}
		return results;
	},
	detachedJobRetention: (api, request) =>
		cases(request).map((value) => {
			const input = value as { candidates: unknown[]; policy: unknown };
			try {
				return {
					ok: true,
					paths: api.selectDetachedArtifactDirectoriesToDelete(input.candidates, input.policy),
				};
			} catch (error) {
				return { ok: false, error: errorOf(error).message };
			}
		}),
	httpDispatcher: (api, request) => ({
		parsed: cases(request).map((value) => {
			const parsed = api.parseHttpIdleTimeoutMs(value);
			return parsed === undefined ? null : parsed;
		}),
		formatted: (request.timeouts as number[]).map((value) => api.formatHttpIdleTimeoutMs(value)),
	}),
	runtimeProcess: (api, request) =>
		cases(request).map((value) => api.commandLineIsPiRuntime(value)),
	runtimeIdentitySnapshot: (api, request) => cases(request).map((pid) => ({
		identity: api.readProcessIdentity(pid),
		group: api.readProcessGroupId(pid),
		alive: api.isProcessIdentityAlive(api.readProcessIdentity(pid)),
	})),
	runtimeIdentityError: (api, request) => cases(request).map((pid) => {
		try {
			return { identity: api.readProcessIdentity(pid) };
		} catch (error) {
			return { error: errorOf(error).message };
		}
	}),
	supersededRuntimeIdentity: (api, request) => cases(request).map((entry) => {
		const [current, replacement] = entry as unknown[];
		return api.isSupersededRuntimeIdentity(current, replacement);
	}),
	activityDetail: (api, request) => cases(request).map((args) => api.summarizeToolArguments(args) ?? null),
	detachedControlEnqueue: (api, request) => request.response
		? api.enqueueDetachedJobStatusResponse(request.input)
		: api.enqueueDetachedJobStatusRequest(request.input),
	detachedControlClaim: async (api, request) => {
		const dbModule = await import(pathToFileURL(`${process.argv[2]}/core/session-control-db.ts`).href);
		dbModule.registerRuntimeMailboxListener(request.db, request.recipient, process.pid);
		return api.claimDetachedJobRuntimeCommands(request.db, request.recipient, request.identity);
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

