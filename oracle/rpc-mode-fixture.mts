import { appendFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

type FixtureResources = { getExtensions(): unknown };
type RpcContext = {
	mode: string;
	ui: {
		select(
			title: string,
			options: string[],
			opts?: { timeout?: number },
		): Promise<string | undefined>;
		confirm(title: string, message: string): Promise<boolean>;
		input(title: string): Promise<string | undefined>;
		editor(title: string): Promise<string | undefined>;
		setStatus(
			key: string,
			status: (view: undefined) => string | undefined,
		): void;
	};
};
type FixtureApi = {
	registerCommand(
		name: string,
		command: { handler(args: string, context: RpcContext): Promise<void> },
	): void;
	on(
		event: "user_bash" | "session_shutdown",
		handler: (event: unknown, context: RpcContext) => Promise<void>,
	): void;
	on(
		event: "session_start",
		handler: (event: unknown, context: RpcContext) => Promise<void>,
	): void;
};

export async function configureRpcResources(
	resources: FixtureResources,
	source: string,
	cwd: string,
) {
	const encoded = process.env.PI_HARNESS_RPC_FIXTURE;
	if (encoded === undefined) return;
	const config = JSON.parse(encoded) as {
		status?: "value" | "omit" | "throw";
		bashHook?: boolean;
		stdoutProbe?: boolean;
		earlySignal?: "SIGTERM" | "SIGHUP";
		pendingShutdown?: boolean;
		uiProbe?: boolean;
		uiTimeout?: number;
	};
	const root = process.env.PI_HARNESS_RPC_ROOT;
	if (root === undefined) throw new Error("RPC fixture root required");
	const loader = await import(
		pathToFileURL(join(source, "core/extensions/loader.ts")).href
	);
	const events = await import(
		pathToFileURL(join(source, "core/event-bus.ts")).href
	);
	const outputGuard = await import(
		pathToFileURL(join(source, "core/output-guard.ts")).href
	);
	const runtime = loader.createExtensionRuntime();
	const extension = await loader.loadExtensionFromFactory(
		(api: FixtureApi) => {
			if (config.uiProbe)
				api.registerCommand("rpc-ui-fixture", {
					handler: async (_args, context) => {
						const selected = await context.ui.select("select", ["one", "two"], {
							timeout: config.uiTimeout,
						});
						writeFileSync(
							join(root, "ui-selected.json"),
							JSON.stringify({ value: selected ?? null }),
						);
						const confirmed = await context.ui.confirm("confirm", "message");
						const input = await context.ui.input("input");
						const editor = await context.ui.editor("editor");
						writeFileSync(
							join(root, "ui-report.json"),
							JSON.stringify({ values: [selected, confirmed, input, editor] }),
						);
					},
				});
			if (config.earlySignal)
				api.on("session_shutdown", async () => {
					writeFileSync(
						join(root, "rpc-shutdown.json"),
						JSON.stringify({ disposed: true }),
					);
					if (config.pendingShutdown) {
						writeFileSync(
							join(root, "rpc-shutdown-ready.json"),
							JSON.stringify({ pid: process.pid }),
						);
						await new Promise<void>(() => {});
					}
				});
			if (config.bashHook)
				api.on("user_bash", async () => {
					appendFileSync(join(root, "bash-hook.jsonl"), "called\n");
				});
			api.on("session_start", async (_event, context) => {
				if (context.mode !== "rpc") return;
				const signal = config.earlySignal;
				if (signal) process.nextTick(() => process.kill(process.pid, signal));
				if (config.stdoutProbe)
					writeFileSync(
						join(root, "stdout-probe.json"),
						JSON.stringify({ takenOver: outputGuard.isStdoutTakenOver() }),
					);
				if (config.status === undefined) return;
				try {
					context.ui.setStatus("callable-status", (view) => {
						appendFileSync(
							join(root, "status-calls.jsonl"),
							`${JSON.stringify(view ?? null)}\n`,
						);
						if (config.status === "throw")
							throw Object.assign(new TypeError("rpc-status-\ud800"), {
								code: "ESTATUS",
							});
						return config.status === "omit" ? undefined : "rpc-value-\udc00";
					});
					writeFileSync(
						join(root, "status-report.json"),
						JSON.stringify({ returned: true }),
					);
				} catch (error) {
					if (!(error instanceof Error)) throw error;
					writeFileSync(
						join(root, "status-report.json"),
						JSON.stringify({
							rejected: Object.assign(
								{ name: error.name, message: error.message },
								error,
							),
						}),
					);
					throw error;
				}
			});
		},
		cwd,
		events.createEventBus(),
		runtime,
		"<rpc-fixture>",
	);
	resources.getExtensions = () => ({
		extensions: [extension],
		errors: [],
		runtime,
	});
}
