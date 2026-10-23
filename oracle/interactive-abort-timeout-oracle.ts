import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { InteractiveMode } = await import(
		pathToFileURL(`${source}/modes/interactive/interactive-mode.ts`).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const originalFetch = globalThis.fetch;
		let guard: ReturnType<typeof setTimeout> | undefined;
		try {
			const { complete } = JSON.parse(line) as { complete: boolean };
			delete process.env.PI_OFFLINE;
			process.env.PI_TELEMETRY = "1";
			let held: AbortSignal | undefined;
			const observed = new Promise<unknown>((resolve, reject) => {
				guard = setTimeout(
					() => reject(new Error("source timeout did not fire")),
					6500,
				);
				globalThis.fetch = ((
					_url: unknown,
					options: { signal: AbortSignal },
				) => {
					const signal = options.signal;
					held = signal;
					const capture = () => {
						const reason = signal.reason;
						resolve({
							aborted: signal.aborted,
							kind:
								reason instanceof DOMException
									? "DOMException"
									: reason instanceof Error
										? "Error"
										: typeof reason,
							name: reason?.name,
							message: reason?.message,
							code: reason?.code,
						});
					};
					signal.addEventListener("abort", capture, { once: true });
					if (complete) return Promise.resolve(undefined);
					return new Promise((_resolve, rejectFetch) =>
						signal.addEventListener("abort", () => rejectFetch(signal.reason), {
							once: true,
						}),
					);
				}) as typeof fetch;
			});
			InteractiveMode.prototype.reportInstallTelemetry.call(
				{ settingsManager: { getEnableInstallTelemetry: () => true } },
				"abort-proof",
			);
			const output = await observed;
			if (!held?.aborted)
				throw new Error("retained source signal was not aborted");
			process.stdout.write(`${JSON.stringify({ ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		} finally {
			if (guard) clearTimeout(guard);
			globalThis.fetch = originalFetch;
		}
	}
}
void main();
