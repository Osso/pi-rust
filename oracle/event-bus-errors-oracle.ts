import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createEventBus(): {
		on(channel: string, handler: () => void): () => void;
		emit(channel: string, data: unknown): void;
	};
};
type Case = { kind: "error" | "value"; value: unknown };
function snapshot(value: unknown) {
	if (!(value instanceof Error)) return { kind: "value", value };
	return {
		kind: "error",
		value: {
			name: value.name,
			message: value.message,
			...Object.fromEntries(
				Object.entries(value).filter(([key]) => key !== "stack"),
			),
		},
	};
}

void runOracle(async (module, request) => {
	if (typeof request.channel !== "string")
		throw new Error("event bus fixture requires a channel");
	const outputs = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		const reason =
			input.kind === "error"
				? Object.assign(new TypeError(input.value as string), {
						code: "E_FIXTURE",
					})
				: input.value;
		const logs: unknown[] = [];
		const previous = console.error;
		console.error = (prefix: unknown, error: unknown) => {
			logs.push({ prefix, reason: snapshot(error) });
		};
		try {
			const bus = (module as Api).createEventBus();
			bus.on(request.channel, () => {
				throw reason;
			});
			bus.emit(request.channel, null);
			await Promise.resolve();
			outputs.push(logs);
		} finally {
			console.error = previous;
		}
	}
	return outputs;
}).catch((error: unknown) => {
	process.stderr.write(`event bus errors oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
