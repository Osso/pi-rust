import { createInterface } from "node:readline";
import { inspect } from "node:util";

// Native fetch is the source operation; no replacement transport or mock errors.
function ownError(error: unknown): unknown {
	if (!(error instanceof Error)) return error;
	return Object.fromEntries(Object.getOwnPropertyNames(error).map((key) => [key, ownError(Reflect.get(error, key))]));
}

async function execute(url: string) {
	const events = ["fetch"];
	let status: number | null = null;
	try {
		const response = await fetch(url, { signal: AbortSignal.timeout(2000) });
		status = response.status;
		events.push("response", "body");
		await response.text();
		throw new Error("fixture unexpectedly completed");
	} catch (error) {
		events.push("error");
		const stderr = `${inspect(error, { depth: null })}\n`;
		process.stderr.write(stderr);
		return {
			events,
			status,
			typeError: error instanceof TypeError,
			error: error instanceof Error ? { name: error.name, message: error.message, cause: ownError(error.cause) } : ownError(error),
			ownError: ownError(error),
			stderr,
		};
	}
}

// Independent deadline also bounds stdin/protocol failures; shared parent watchdog
// and TsOracle process-group teardown cover a disappearing Rust test process.
async function main(): Promise<void> {
	const deadline = setTimeout(() => process.exit(124), 5000);
	try {
		for await (const line of createInterface({ input: process.stdin })) {
			const { url } = JSON.parse(line) as { url: string };
			process.stdout.write(`${JSON.stringify(await execute(url))}\n`);
		}
	} finally {
		clearTimeout(deadline);
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`${inspect(error)}\n`);
	process.exitCode = 1;
});
