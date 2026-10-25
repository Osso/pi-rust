import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Handle = { detach(): boolean };
type Registry = {
	register(handle: Handle): () => void;
	detachRunning(): boolean;
	detachAll(): void;
	hasRunning(): boolean;
};
type Api = {
	ToolDetachRegistry: new (options?: {
		autoDetachAfterMs?: number;
	}) => Registry;
	DEFAULT_AUTO_DETACH_AFTER_MS: number;
};
type Input = {
	handles: {
		name: string;
		result: boolean;
		throw?: { kind: "error" | "value"; value: unknown };
	}[];
	steps: { op: string; name?: string; index?: number }[];
};
function snapshot(error: unknown) {
	return error instanceof Error
		? {
				kind: "error",
				value: {
					name: error.name,
					message: error.message,
					...Object.fromEntries(Object.entries(error)),
				},
			}
		: { kind: "value", value: error };
}
function runHistory(api: Api, input: Input) {
	const registry = new api.ToolDetachRegistry();
	const calls: string[] = [];
	const unsubscribe: (() => void)[] = [];
	const handles = new Map(
		input.handles.map((input) => [
			input.name,
			{
				detach: () => {
					calls.push(input.name);
					if (input.throw) {
						if (input.throw.kind === "value") throw input.throw.value;
						if (typeof input.throw.value !== "string")
							throw new Error("fixture error message required");
						throw Object.assign(new TypeError(input.throw.value), {
							code: "E_DETACH",
						});
					}
					return input.result;
				},
			},
		]),
	);
	try {
		return input.steps.map((step) => {
			let result: boolean | null = null;
			let error: unknown;
			try {
				switch (step.op) {
					case "register": {
						const handle = handles.get(step.name ?? "");
						if (!handle) throw new Error("fixture handle required");
						unsubscribe.push(registry.register(handle));
						break;
					}
					case "unregister":
						unsubscribe[step.index ?? 0]();
						break;
					case "running":
						result = registry.detachRunning();
						break;
					case "all":
						registry.detachAll();
						break;
					case "has":
						break;
					default:
						throw new Error("unknown fixture step");
				}
			} catch (thrown) {
				error = snapshot(thrown);
			}
			return {
				op: step.op,
				result,
				calls: [...calls],
				hasRunning: registry.hasRunning(),
				...(error === undefined ? {} : { error }),
			};
		});
	} finally {
		for (const unregister of unsubscribe) unregister();
	}
}
async function retainedSubscriptionTimer(api: Api, delay = 20) {
	let unregister: (() => void) | undefined;
	let guard: ReturnType<typeof setTimeout> | undefined;
	const fired = new Promise<boolean>((resolve) => {
		const registry = new api.ToolDetachRegistry({ autoDetachAfterMs: delay });
		unregister = registry.register({
			detach: () => {
				resolve(true);
				return true;
			},
		});
	});
	try {
		const called = await Promise.race([
			fired,
			new Promise<boolean>((resolve) => {
				guard = setTimeout(() => resolve(false), 2000);
			}),
		]);
		return { called };
	} finally {
		if (guard) clearTimeout(guard);
		if (unregister) unregister();
	}
}
void runOracle(async (module, request) => {
	const api = module as Api;
	if (request.operation === "timerThrow") {
		const value = request.reason as { kind: "error" | "value"; value: unknown };
		const reason =
			value.kind === "error"
				? Object.assign(new TypeError(String(value.value)), {
						code: "E_DETACH",
					})
				: value.value;
		process.once("uncaughtExceptionMonitor", (error) => {
			process.stdout.write(
				`FROZEN_TIMER_THROW=${JSON.stringify(snapshot(error))}\n`,
			);
		});
		const registry = new api.ToolDetachRegistry({ autoDetachAfterMs: 1 });
		registry.register({
			detach: () => {
				throw reason;
			},
		});
		await new Promise<void>((resolve) => setTimeout(resolve, 2000));
		throw new Error("unref'ed timer unexpectedly survived its throw");
	}
	if (request.operation === "retainedSubscriptionTimer")
		return retainedSubscriptionTimer(api);
	if (request.operation === "timerDelay") {
		if (typeof request.delay !== "number")
			throw new Error("numeric timer fixture required");
		return retainedSubscriptionTimer(api, request.delay);
	}
	return {
		defaultDelay: api.DEFAULT_AUTO_DETACH_AFTER_MS,
		histories: requireCases(request).map((input) =>
			runHistory(api, input as Input),
		),
	};
}).catch((error: unknown) => {
	process.stderr.write(`tool detach oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
