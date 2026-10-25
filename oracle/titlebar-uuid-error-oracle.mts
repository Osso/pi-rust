import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Context = { ui: { setTitle(title: string): void } };
type Handler = (event: unknown, context: Context) => Promise<void>;
type Api = {
	on(event: string, handler: Handler): void;
	getSessionName(): undefined;
};
const module = (await import(pathToFileURL(process.argv[2]).href)) as {
	default(api: Api): void;
};
for await (const _line of createInterface({ input: process.stdin })) {
	let calls = 0;
	const handlers = new Map<string, Handler>();
	process.once("uncaughtException", (error: Error & { code?: string }) => {
		console.log(
			JSON.stringify({
				boundary: "uncaughtException",
				calls,
				error: { name: error.name, message: error.message, code: error.code },
			}),
		);
		process.exit(0);
	});
	const context: Context = {
		ui: {
			setTitle() {
				calls++;
				if (calls === 2)
					throw Object.assign(new Error("Deriving bits failed"), {
						code: "ERR_CRYPTO_OPERATION_FAILED",
					});
			},
		},
	};
	module.default({
		on: (event, handler) => {
			handlers.set(event, handler);
		},
		getSessionName: () => undefined,
	});
	const start = handlers.get("agent_start");
	if (!start) throw new Error("source did not register agent_start");
	setTimeout(() => {
		console.log(JSON.stringify({ boundary: "watchdog", calls }));
		process.exit(2);
	}, 2000);
	await start({}, context);
}
