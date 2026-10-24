import { appendFileSync, existsSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

type Rejection =
	| { kind: "raw"; value: unknown }
	| {
			kind: "error";
			name: "Error" | "TypeError";
			message: string;
			code?: string;
	  };
type Config = {
	capture?: boolean;
	traceDispose?: boolean;
	disposeRejection?: Rejection;
	promptRejection?: Rejection;
	reloadProbe?: { message: string; fifo?: boolean; success?: boolean };
	rebindProbe?: boolean;
	earlySignal?: "SIGTERM" | "SIGHUP";
	pendingShutdown?: boolean;
};
type Runtime = {
	reload(): Promise<void>;
	setBeforeSessionInvalidate(handler: () => void): void;
	session: {
		setWorkingDirectoryRecoveryHandlers(handlers: {
			beforeRun(): Promise<never>;
			afterTurn(): Promise<void>;
		}): void;
	};
};

function makeRejection(value: Rejection): unknown {
	if (value.kind === "raw") return value.value;
	const error =
		value.name === "TypeError"
			? new TypeError(value.message)
			: new Error(value.message);
	if (value.code !== undefined) Object.assign(error, { code: value.code });
	return error;
}

function serializeRejection(error: unknown) {
	if (!(error instanceof Error)) return { kind: "raw", value: error };
	return {
		kind: "error",
		value: Object.assign({ name: error.name, message: error.message }, error),
		typeError: error instanceof TypeError,
	};
}

let reloadArmed = false;
let reloadRuntime: Runtime | undefined;

type FixtureResources = { getExtensions(): unknown; reload(): Promise<void> };
type ReloadContext = {
	reload(): Promise<void>;
	isIdle(): boolean;
	newSession(): Promise<unknown>;
};
type FixtureApi = {
	on(
		event: "session_start" | "session_shutdown",
		handler: () => Promise<void>,
	): void;
	registerCommand(
		name: string,
		options: { handler(args: string, context: ReloadContext): Promise<void> },
	): void;
};

export async function configurePrintResources(
	resources: FixtureResources,
	source: string,
	cwd: string,
) {
	const encoded = process.env.PI_HARNESS_PRINT_FIXTURE;
	if (encoded === undefined) return;
	const config = JSON.parse(encoded) as Config;
	if (
		config.reloadProbe === undefined &&
		config.rebindProbe !== true &&
		config.earlySignal === undefined &&
		config.pendingShutdown !== true
	)
		return;
	const root = process.env.PI_HARNESS_PRINT_ROOT;
	if (root === undefined) throw new Error("print fixture root required");
	const loader = await import(
		pathToFileURL(join(source, "core/extensions/loader.ts")).href
	);
	const events = await import(
		pathToFileURL(join(source, "core/event-bus.ts")).href
	);
	const runtime = loader.createExtensionRuntime();
	const extension = await loader.loadExtensionFromFactory(
		(api: FixtureApi) => {
			if (config.pendingShutdown === true)
				api.on("session_shutdown", async () => {
					writeFileSync(join(root, "print-shutdown-ready"), "ready");
					await new Promise<never>(() => {});
				});
			api.registerCommand("print-rebind-probe", {
				handler: async (_args, context) => {
					await context.newSession();
				},
			});
			if (config.earlySignal !== undefined)
				api.on("session_start", async () => {
					if (!reloadArmed) return;
					setInterval(() => {}, 1000);
					process.kill(process.pid, config.earlySignal);
					await new Promise<never>(() => {});
				});
			api.registerCommand("print-reload-probe", {
				handler: async (_args, context) => {
					try {
						let first: Promise<unknown> | undefined;
						if (config.reloadProbe?.fifo === true) {
							if (reloadRuntime === undefined)
								throw new Error("reload runtime required");
							first = reloadRuntime.reload().catch((error) => error);
							while (!existsSync(join(root, "print-reload-ready.json")))
								await new Promise((resolve) => setTimeout(resolve, 5));
						}
						try {
							await context.reload();
						} finally {
							await first;
						}
						writeFileSync(
							join(root, "print-reload-outcome.json"),
							JSON.stringify(
								config.reloadProbe?.success === true
									? { returned: true, oldContextIdle: context.isIdle() }
									: { returned: true },
							),
						);
					} catch (error) {
						writeFileSync(
							join(root, "print-reload-outcome.json"),
							JSON.stringify({ rejected: serializeRejection(error) }),
						);
					}
				},
			});
		},
		cwd,
		events.createEventBus(),
		runtime,
		"<print-fixture>",
	);
	resources.getExtensions = () => ({
		extensions: [extension],
		errors: [],
		runtime: loader.createExtensionRuntime(),
	});
	if (config.reloadProbe === undefined) return;
	const message = config.reloadProbe.message;
	resources.reload = async () => {
		if (!reloadArmed) return;
		appendFileSync(
			join(root, "print-reload-entries.jsonl"),
			'{"entered":true}\n',
		);
		writeFileSync(
			join(root, "print-reload-ready.pending"),
			JSON.stringify({ pid: process.pid }),
		);
		renameSync(
			join(root, "print-reload-ready.pending"),
			join(root, "print-reload-ready.json"),
		);
		while (!existsSync(join(root, "print-reload-release")))
			await new Promise((resolve) => setTimeout(resolve, 5));
		if (config.reloadProbe?.success !== true) throw new Error(message);
	};
}

type ObservedSession = {
	subscribe(handler: (event: { type: string }) => void): unknown;
	agent: { subscribe(handler: (event: { type: string }) => void): unknown };
};

export function observePrintReplacement(session: ObservedSession) {
	const encoded = process.env.PI_HARNESS_PRINT_FIXTURE;
	if (
		encoded === undefined ||
		(JSON.parse(encoded) as Config).rebindProbe !== true
	)
		return;
	const root = process.env.PI_HARNESS_PRINT_ROOT;
	if (root === undefined) throw new Error("print fixture root required");
	for (const [name, emitter] of [
		["session", session],
		["agent", session.agent],
	] as const) {
		emitter.subscribe((event) => {
			if (event.type === "agent_end")
				writeFileSync(
					join(root, `print-observer-${name}.json`),
					JSON.stringify({ observed: true }),
				);
		});
	}
}

export function markPrintPending() {
	if (process.env.PI_HARNESS_PRINT_FIXTURE === undefined) return;
	const root = process.env.PI_HARNESS_PRINT_ROOT;
	if (root === undefined) throw new Error("print fixture root required");
	// A live provider holds a socket; this private pending factory holds a timer instead.
	setInterval(() => {}, 1000);
	writeFileSync(
		join(root, "print-ready.pending"),
		JSON.stringify({ pid: process.pid }),
	);
	renameSync(join(root, "print-ready.pending"), join(root, "print-ready.json"));
}

export function configurePrintFixture(runtime: Runtime) {
	const encoded = process.env.PI_HARNESS_PRINT_FIXTURE;
	if (encoded === undefined) return undefined;
	const root = process.env.PI_HARNESS_PRINT_ROOT;
	if (root === undefined) throw new Error("print fixture root required");
	const config = JSON.parse(encoded) as Config;
	reloadArmed = true;
	reloadRuntime = runtime;
	if (config.promptRejection !== undefined) {
		const rejection = makeRejection(config.promptRejection);
		runtime.session.setWorkingDirectoryRecoveryHandlers({
			beforeRun: async () => {
				throw rejection;
			},
			afterTurn: async () => {},
		});
	}
	if (config.disposeRejection !== undefined || config.traceDispose === true) {
		const rejection =
			config.disposeRejection === undefined
				? undefined
				: makeRejection(config.disposeRejection);
		runtime.setBeforeSessionInvalidate(() => {
			appendFileSync(
				join(root, "print-trace.jsonl"),
				`${JSON.stringify({ event: "beforeInvalidate" })}\n`,
			);
			if (config.disposeRejection !== undefined) throw rejection;
		});
	}
	return {
		capture: config.capture === true,
		returned(code: number) {
			writeFileSync(
				join(root, "print-report.json"),
				JSON.stringify({ returned: code }),
			);
		},
		rejected(error: unknown) {
			writeFileSync(
				join(root, "print-report.json"),
				JSON.stringify({ rejected: serializeRejection(error) }),
			);
		},
	};
}
