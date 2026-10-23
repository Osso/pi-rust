import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";
import type { SessionTreeEntry } from "/home/osso/.worktrees/pi-rust-source-4f9e6f8a4/packages/agent-core/src/harness/types.ts";

interface Input {
	root: string;
	action: string;
	entries: SessionTreeEntry[];
	message: string;
	rejected: boolean;
}
function errorValue(error: unknown): unknown {
	if (!(error instanceof Error)) throw error;
	const fields = error as Error & { code?: string; errors?: Error[] };
	return {
		name: error.name,
		message: error.message,
		code: fields.code,
		...(error.cause === undefined ? {} : { cause: errorValue(error.cause) }),
		...(fields.errors === undefined
			? {}
			: { errors: fields.errors.map(errorValue) }),
	};
}
async function main(): Promise<void> {
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("missing source module");
	const source = pathToFileURL(sourcePath);
	const { AgentHarness } = await import(source.href);
	const { JsonlSessionStorage } = await import(
		new URL("./session/jsonl-storage.ts", source).href
	);
	const { Session } = await import(
		new URL("./session/session.ts", source).href
	);
	const { NodeExecutionEnv } = await import(
		new URL("./env/nodejs.ts", source).href
	);
	const { FileError, AgentHarnessError } = await import(
		new URL("./types.ts", source).href
	);
	const root = resolve(dirname(sourcePath), "../../../..");
	const { createModels, fauxProvider, fauxAssistantMessage } = await import(
		pathToFileURL(resolve(root, "packages/ai/src/index.ts")).href
	);
	globalThis.fetch = async () => {
		throw new Error("unexpected oracle HTTP request");
	};
	for await (const line of createInterface({ input: process.stdin })) {
		const input: Input = JSON.parse(line);
		const env = new NodeExecutionEnv({ cwd: input.root });
		const storage = await JsonlSessionStorage.create(
			env,
			resolve(input.root, "session.jsonl"),
			{ cwd: "/fixture", sessionId: "fixture" },
		);
		for (const entry of input.entries) await storage.appendEntry(entry);
		const session = new Session(storage);
		const models = createModels();
		const faux = fauxProvider({ api: "fixture-api", provider: "fixture" });
		models.setProvider(faux.provider);
		const harness = new AgentHarness({
			env,
			session,
			models,
			model: faux.getModel(),
		});
		if (input.action.startsWith("callback-")) {
			let observed: unknown;
			const callbackError = () =>
				new AgentHarnessError(
					"auth",
					input.message,
					input.rejected ? new TypeError("cause\udc00") : undefined,
				);
			if (input.action === "callback-payload")
				harness.on("before_provider_payload", () => {
					throw callbackError();
				});
			else
				harness.subscribe((event: { type: string }) => {
					if (event.type === "after_provider_response") throw callbackError();
				});
			type CapturedOptions = {
				onPayload: (payload: unknown, model: unknown) => Promise<unknown>;
				onResponse: (response: unknown, model: unknown) => Promise<unknown>;
			};
			let captured: CapturedOptions | undefined;
			const original = models.streamSimple.bind(models);
			models.streamSimple = (
				model: unknown,
				context: unknown,
				options: CapturedOptions,
			) => {
				captured = options;
				return original(model, context, options);
			};
			faux.setResponses([fauxAssistantMessage("done")]);
			await harness.prompt("prompt");
			if (!captured) throw new Error("provider options were not captured");
			try {
				if (input.action === "callback-payload")
					await captured.onPayload({}, faux.getModel());
				else
					await captured.onResponse(
						{ status: 201, headers: { "x-fixture": "raw" } },
						faux.getModel(),
					);
			} catch (error) {
				observed = errorValue(error);
			}
			if (observed === undefined)
				throw new Error("callback fixture did not observe rejection");
			console.log(JSON.stringify(observed));
			continue;
		}
		if (input.action === "aggregate") {
			harness.on("before_provider_request", () => {
				throw new AgentHarnessError("auth", input.message);
			});
			harness.subscribe(
				(event: { type: string; message?: { role: string } }) => {
					if (
						event.type === "message_start" &&
						event.message?.role === "assistant"
					)
						throw new AgentHarnessError("hook", "report");
				},
			);
		} else {
			env.appendFile = async () => {
				if (input.rejected) throw new TypeError(input.message);
				return {
					ok: false,
					error: new FileError("permission_denied", input.message),
				};
			};
			harness.on("session_before_compact", () => ({
				compaction: {
					summary: "provided",
					firstKeptEntryId: "v",
					tokensBefore: 12,
				},
			}));
		}
		let result: unknown;
		try {
			if (input.action === "compact") await harness.compact();
			else if (input.action === "navigate") await harness.navigateTree("a");
			else await harness.prompt("prompt");
			result = { ok: true };
		} catch (error) {
			result = { ok: false, error: errorValue(error) };
		}
		console.log(JSON.stringify(result));
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
