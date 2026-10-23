// Drive the actual frozen helper with native Node controllers; snapshots omit only JS stacks.
import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

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

function snapshot(signal: AbortSignal | undefined): unknown {
	return signal ? { aborted: signal.aborted, reason: describeAbortReason(signal.reason) } : { kind: "no-signal" };
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
