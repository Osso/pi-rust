// Drive the actual frozen helper with native Node controllers; snapshots omit only JS stacks.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

type Descriptor = { kind: string; value?: unknown; name?: string; message?: string; properties?: Record<string, unknown> };
type Action = { kind: "abort" | "cleanup"; index?: number; reason?: Descriptor };
type Combined = { signal?: AbortSignal; cleanup(): void };
type Combine = (signals: Array<AbortSignal | undefined>) => Combined;

function decode(reason: Descriptor): unknown {
	if (reason.kind === "default") return undefined;
	if (reason.kind === "value") return reason.value;
	if (reason.kind === "number") return Number(reason.value);
	const error = reason.kind === "dom"
		? new DOMException(reason.message, reason.name)
		: Object.assign(new Error(reason.message), { name: reason.name ?? "Error" });
	return Object.assign(error, reason.properties);
}

function errorRecord(error: Error): Record<string, unknown> {
	const result: Record<string, unknown> = { name: error.name, message: error.message };
	const properties = error as unknown as Record<string, unknown>;
	for (const key of ["code", "errno", "syscall", "path", "cause", ...Object.keys(error)]) {
		if (key === "stack" || key === "name" || key === "message" || properties[key] === undefined) continue;
		const value = properties[key];
		result[key] = value instanceof Error ? errorRecord(value) : value;
	}
	return result;
}

function reasonValue(reason: unknown): unknown {
	if (reason === undefined) return { kind: "undefined" };
	if (reason instanceof Error) return { kind: reason instanceof DOMException ? "dom" : "error", value: errorRecord(reason) };
	if (typeof reason === "number") return { kind: "number", value: Object.is(reason, -0) ? "-0" : String(reason) };
	return { kind: "value", value: reason };
}

function snapshot(signal: AbortSignal | undefined): unknown {
	return signal ? { aborted: signal.aborted, reason: reasonValue(signal.reason) } : { kind: "no-signal" };
}

function observe(combine: Combine, input: Record<string, unknown>): unknown {
	const present = input.present as boolean[];
	const initial = input.initial as Array<Descriptor | null>;
	const controllers = present.map((exists, index) => {
		if (!exists) return undefined;
		const controller = new AbortController();
		const reason = initial[index];
		if (reason) controller.abort(decode(reason));
		return controller;
	});
	const sources = controllers.map((controller) => controller?.signal);
	let combined: Combined | undefined = combine(sources);
	const signal = combined.signal;
	const sameSources = sources.map((source) => source !== undefined && source === signal);
	const snapshots = [snapshot(signal)];
	if (input.dropWrapper) combined = undefined;
	for (const action of input.actions as Action[]) {
		if (action.kind === "cleanup") {
			if (!combined) throw new Error("fixture cannot clean up a discarded wrapper");
			combined.cleanup();
		} else {
			controllers[action.index!]!.abort(decode(action.reason!));
		}
		snapshots.push(snapshot(signal));
	}
	return { sameSources, snapshots };
}

async function main(): Promise<void> {
	const path = process.argv[2];
	if (!path) throw new Error("combine oracle requires a frozen module path");
	const api = await import(pathToFileURL(path).href) as { combineAbortSignals: Combine };
	serve((input) => observe(api.combineAbortSignals, input));
}
void main().catch((error: unknown) => {
	process.stderr.write(`combine oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
