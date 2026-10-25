import {
	runOracle,
	requireCases,
} from "./coding-agent-utils-oracle-support.ts";

type Accumulator = { consume(records: unknown[]): unknown; close(): void };
runOracle((api, request) => {
	const oldSet = globalThis.setTimeout;
	const oldClear = globalThis.clearTimeout;
	let tick: (() => void) | undefined;
	globalThis.setTimeout = ((callback: () => void) => {
		tick = callback;
		return 1;
	}) as unknown as typeof setTimeout;
	globalThis.clearTimeout = (() => {
		tick = undefined;
	}) as typeof clearTimeout;
	try {
		return requireCases(request).map((value) => {
			const steps = value as {
				records?: unknown[];
				flush?: boolean;
				close?: boolean;
			}[];
			const events: unknown[] = [];
			const accumulator = (
				api.createArtifactProgressAccumulator as (
					report: (update: unknown) => void,
					delay: number,
				) => Accumulator
			)((update) => events.push(update), 25);
			const snapshots = steps.map((step) => {
				const result = step.records
					? accumulator.consume(step.records)
					: undefined;
				if (step.flush) {
					const callback = tick;
					tick = undefined;
					callback?.();
				}
				if (step.close) accumulator.close();
				return { events: [...events], result: result ?? null };
			});
			accumulator.close();
			return snapshots;
		});
	} finally {
		globalThis.setTimeout = oldSet;
		globalThis.clearTimeout = oldClear;
	}
}).catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
