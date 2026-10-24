import { appendFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

type FixtureResources = { getExtensions(): unknown };
type RpcContext = {
	mode: string;
	ui: {
		setStatus(
			key: string,
			status: (view: undefined) => string | undefined,
		): void;
	};
};
type FixtureApi = {
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
	const config = JSON.parse(encoded) as { status: "value" | "omit" | "throw" };
	const root = process.env.PI_HARNESS_RPC_ROOT;
	if (root === undefined) throw new Error("RPC fixture root required");
	const loader = await import(
		pathToFileURL(join(source, "core/extensions/loader.ts")).href
	);
	const events = await import(
		pathToFileURL(join(source, "core/event-bus.ts")).href
	);
	const runtime = loader.createExtensionRuntime();
	const extension = await loader.loadExtensionFromFactory(
		(api: FixtureApi) => {
			api.on("session_start", async (_event, context) => {
				if (context.mode !== "rpc") return;
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
