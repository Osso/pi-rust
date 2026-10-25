import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [source, root, order] = process.argv.slice(2);
if (!source || !root || !order) throw new Error("source/root/order required");
const controller = new AbortController();
let ui: {
	select(
		title: string,
		options: string[],
		opts: { signal: AbortSignal },
	): Promise<string | undefined>;
};
const rawWrite = process.stdout.write.bind(process.stdout);
process.stdout.write = ((
	chunk: string,
	callback?: (error?: Error | null) => void,
) => {
	const request = JSON.parse(chunk) as {
		type: string;
		method?: string;
		id?: string;
	};
	if (request.type === "extension_ui_request" && request.method === "select") {
		const reply = () =>
			process.stdin.emit(
				"data",
				Buffer.from(
					`${JSON.stringify({ type: "extension_ui_response", id: request.id, value: "answered" })}\n`,
				),
			);
		if (order === "response-first") {
			reply();
			controller.abort();
		} else {
			controller.abort();
			reply();
		}
	} else {
		rawWrite(chunk);
	}
	callback?.(null);
	return true;
}) as typeof process.stdout.write;
const session = {
	bindExtensions: async (options: { uiContext: typeof ui }) => {
		ui = options.uiContext;
	},
	subscribe: () => () => {},
	agent: { subscribe: () => () => {} },
	messages: [],
	consumeResumeContinuationRequest: () => false,
};
const runtime = {
	session,
	setRebindSession: () => {},
	dispose: async () => {},
};
const mode = await import(
	pathToFileURL(join(source, "modes/rpc/rpc-mode.ts")).href
);
setImmediate(async () => {
	const value = await ui.select("ordered", ["answered"], {
		signal: controller.signal,
	});
	writeFileSync(
		join(root, "dialog-order.json"),
		JSON.stringify({ value: value ?? null }),
	);
	process.exit(0);
});
await mode.runRpcMode(runtime);
