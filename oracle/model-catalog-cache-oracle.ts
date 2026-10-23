// Real frozen catalog module; only HTTP is scripted. Files and public return values stay literal.
import { pathToFileURL } from "node:url";
import { createRequire, syncBuiltinESMExports } from "node:module";
const timers = createRequire(import.meta.url)("node:timers/promises") as { setTimeout: typeof import("node:timers/promises").setTimeout };
import { serve } from "./config-auth-oracle-support.ts";
import { materializeThrownValue, snapshotThrownValue, type ThrownValueSpec } from "./thrown-value-support.ts";

type CatalogApi = {
	loadOpenRouterCatalogAtStartup(options: { now: () => Date }): Promise<unknown>;
	ensureModelCatalogFresh(options: { force?: boolean; now: () => Date; fetchImpl: typeof fetch }): Promise<unknown>;
	fetchOpenRouterCatalog(signal?: AbortSignal): Promise<unknown>;
	getOpenRouterCatalogModels(): unknown;
};

async function main(): Promise<void> {
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("catalog oracle requires source path");
	const api = await import(pathToFileURL(sourcePath).href) as CatalogApi;
	serve(async (request) => {
		const calls: unknown[] = [];
		const waits: number[] = [];
		let clockCalls = 0, randomCalls = 0;
		const originalNow = Date.now, originalRandom = Math.random, originalDelay = timers.setTimeout;
		if (request.inspectRetry) {
			const clocks = request.clockValues as number[];
			const random = request.randomValues as number[];
			Date.now = () => clocks[Math.min(clockCalls++, clocks.length - 1)];
			Math.random = () => random[Math.min(randomCalls++, random.length - 1)];
			timers.setTimeout = <T = void>(milliseconds?: number, value?: T, options?: Parameters<typeof originalDelay>[2]) => {
				waits.push(Number(milliseconds));
				return originalDelay<T>(request.realRetryDelay ? milliseconds : 0, value, options);
			};
			syncBuiltinESMExports();
		}
		const retrySnapshot = () => request.inspectRetry ? { waits, clockCalls, randomCalls } : {};
		let lastSignal: AbortSignal | null | undefined;
		const signalSnapshot = () => request.inspectSignal ? { signalFinal: lastSignal ? {
			aborted: lastSignal.aborted, reason: snapshotThrownValue(lastSignal.reason), dom: lastSignal.reason instanceof DOMException,
		} : null } : {};
		const scriptedFetch: typeof fetch = async (input, init) => {
			lastSignal = init?.signal;
			calls.push({ url: String(input), signal: init?.signal instanceof AbortSignal });
			if (request.awaitAbort) {
				const signal = init?.signal;
				if (!signal) throw new Error("abort fixture requires signal");
				await new Promise<never>((_resolve, reject) => {
					if (signal.aborted) reject(signal.reason);
					else signal.addEventListener("abort", () => reject(signal.reason), { once: true });
				});
			}
			if (typeof request.delayMs === "number") await new Promise((resolve) => setTimeout(resolve, request.delayMs as number));
			if (request.rejectSignalReason) throw init?.signal?.reason;
			if (request.failureSpec) throw materializeThrownValue(request.failureSpec as ThrownValueSpec);
			if (request.bodyFailureSpec) {
				return new Response(new ReadableStream({ start(controller) {
					controller.error(materializeThrownValue(request.bodyFailureSpec as ThrownValueSpec));
				} }));
			}
			if (typeof request.failure === "string") {
				throw new TypeError(request.failure, { cause: request.failureCause });
			}
			return new Response(typeof request.body === "string" ? request.body : JSON.stringify(request.payload), {
				status: Number(Array.isArray(request.statuses)
					? request.statuses[Math.min(calls.length - 1, request.statuses.length - 1)]
					: request.status ?? 200),
				headers: (request.headers ?? undefined) as Record<string, string> | undefined,
			});
		};
		globalThis.fetch = scriptedFetch;
		const now = () => new Date(Number(request.now));
		const caller = request.operation === "fetch" && (request.abortDefault || request.abortSpec) ? new AbortController() : undefined;
		if (caller) {
			if (request.abortDefault) caller.abort();
			else caller.abort(materializeThrownValue(request.abortSpec as ThrownValueSpec));
		}
		const invoke = () => request.operation === "startup"
			? api.loadOpenRouterCatalogAtStartup({ now })
			: request.operation === "fetch"
				? api.fetchOpenRouterCatalog(caller?.signal)
				: api.ensureModelCatalogFresh({ now, force: request.force as boolean | undefined, fetchImpl: scriptedFetch });
		try {
			const result = await invoke();
			const numericStates = request.inspectNumbers && Array.isArray(result)
				? result.map((model: { cost: { input: number } }) => ({
					type: typeof model.cost.input,
					finite: Number.isFinite(model.cost.input),
					negativeZero: Object.is(model.cost.input, -0),
				}))
				: undefined;
			const models = (result as { models?: Array<{ id: string; autoCompactionThreshold?: number }> }).models;
			const inspectedModel = models?.find((model) => model.id === request.inspectId);
			const modelWire = typeof request.inspectId === "string" ? JSON.stringify(inspectedModel) : undefined;
			const optionalNumberState = request.inspectOptional && inspectedModel
				? {
					present: "autoCompactionThreshold" in inspectedModel,
					type: typeof inspectedModel.autoCompactionThreshold,
					finite: Number.isFinite(inspectedModel.autoCompactionThreshold),
				}
				: undefined;
			return { result, calls, memoized: api.getOpenRouterCatalogModels(), numericStates, modelWire, optionalNumberState, ...signalSnapshot(), ...retrySnapshot() };
		} catch (error) {
			if (!request.inspectError) throw error;
			return { error: snapshotThrownValue(error), calls, ...signalSnapshot(), ...retrySnapshot() };
		} finally {
			if (request.inspectRetry) {
				Date.now = originalNow;
				Math.random = originalRandom;
				timers.setTimeout = originalDelay;
				syncBuiltinESMExports();
			}
		}
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`catalog oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
