import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Descriptor = {
	kind: "default" | "value" | "number" | "error" | "dom";
	value?: unknown;
	name?: string;
	message?: string;
	properties?: Record<string, unknown>;
};
type Action = { index: number; reason: Descriptor };
type Request = {
	mode: "controller" | "static" | "any" | "subscriptions" | "timeout";
	first?: Descriptor;
	second?: Descriptor;
	initial?: Array<Descriptor | null>;
	actions?: Action[];
	engine?: "node" | "frozen";
	delay?: string;
	wait?: boolean;
};

function decode(descriptor: Descriptor | undefined): unknown {
	if (!descriptor || descriptor.kind === "default") return undefined;
	if (descriptor.kind === "value") return descriptor.value;
	if (descriptor.kind === "number") return Number(descriptor.value);
	const error =
		descriptor.kind === "dom"
			? new DOMException(descriptor.message, descriptor.name)
			: Object.assign(new Error(descriptor.message), {
					name: descriptor.name ?? "Error",
				});
	return Object.assign(error, descriptor.properties);
}

function errorRecord(error: Error): Record<string, unknown> {
	const record: Record<string, unknown> = {
		name: error.name,
		message: error.message,
	};
	const properties = error as unknown as Record<string, unknown>;
	for (const key of [
		"code",
		"errno",
		"syscall",
		"path",
		"cause",
		...Object.keys(error),
	]) {
		if (
			key === "stack" ||
			key === "name" ||
			key === "message" ||
			properties[key] === undefined
		)
			continue;
		const value = properties[key];
		record[key] = value instanceof Error ? errorRecord(value) : value;
	}
	return record;
}

function reasonValue(reason: unknown): unknown {
	if (reason === undefined) return { kind: "undefined" };
	if (reason instanceof Error) {
		return {
			kind: reason instanceof DOMException ? "dom" : "error",
			value: errorRecord(reason),
		};
	}
	if (typeof reason === "number") {
		const value = Object.is(reason, -0) ? "-0" : String(reason);
		return { kind: "number", value };
	}
	return { kind: "value", value: reason };
}

function snapshot(signal: AbortSignal): unknown {
	return { aborted: signal.aborted, reason: reasonValue(signal.reason) };
}

function observeController(input: Request): unknown {
	const controller = new AbortController();
	const events: unknown[] = [];
	controller.signal.addEventListener(
		"abort",
		() => events.push(reasonValue(controller.signal.reason)),
		{ once: true },
	);
	const before = snapshot(controller.signal);
	controller.abort(decode(input.first));
	const after = snapshot(controller.signal);
	controller.abort(decode(input.second));
	let lateCalled = 0;
	controller.signal.addEventListener("abort", () => lateCalled++, {
		once: true,
	});
	return {
		before,
		after,
		afterSecond: snapshot(controller.signal),
		events,
		lateCalled,
	};
}

type Combine = (signals: AbortSignal[]) => {
	signal?: AbortSignal;
	cleanup: () => void;
};
function observeAny(input: Request, combine: Combine): unknown {
	const controllers = (input.initial ?? []).map((reason) => {
		const controller = new AbortController();
		if (reason) controller.abort(decode(reason));
		return controller;
	});
	const sources = controllers.map((controller) => controller.signal);
	const signal =
		input.engine === "frozen"
			? combine(sources).signal
			: AbortSignal.any(sources);
	if (!signal)
		throw new Error("frozen combine fixture requires at least one signal");
	const snapshots = [snapshot(signal)];
	for (const action of input.actions ?? []) {
		controllers[action.index].abort(decode(action.reason));
		snapshots.push(snapshot(signal));
	}
	return snapshots;
}

function observeSubscriptions(): unknown {
	const controller = new AbortController();
	const events: string[] = [];
	const later = () => events.push("removed");
	controller.signal.addEventListener(
		"abort",
		() => {
			events.push("first");
			controller.signal.removeEventListener("abort", later);
		},
		{ once: true },
	);
	controller.signal.addEventListener("abort", later, { once: true });
	controller.signal.addEventListener("abort", () => events.push("last"), {
		once: true,
	});
	controller.abort("reason");
	controller.abort("ignored");
	return events;
}

async function observeTimeout(input: Request): Promise<unknown> {
	try {
		const signal = AbortSignal.timeout(Number(input.delay));
		const before = snapshot(signal);
		if (!input.wait) return { before };
		await new Promise<void>((resolve) =>
			signal.addEventListener("abort", () => resolve(), { once: true }),
		);
		return { before, after: snapshot(signal) };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return { error: errorRecord(error) };
	}
}

async function observe(input: Request, combine: Combine): Promise<unknown> {
	switch (input.mode) {
		case "controller":
			return observeController(input);
		case "static":
			return snapshot(AbortSignal.abort(decode(input.first)));
		case "any":
			return observeAny(input, combine);
		case "subscriptions":
			return observeSubscriptions();
		case "timeout":
			return observeTimeout(input);
	}
}

async function main(): Promise<void> {
	const source = process.argv[2];
	if (!source) throw new Error("missing frozen abort-signals module path");
	const { combineAbortSignals } = (await import(
		pathToFileURL(source).href
	)) as { combineAbortSignals: Combine };
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line) as Request;
		const result = await observe(input, combineAbortSignals);
		console.log(JSON.stringify(result));
	}
}
main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
