import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const { lazyStream } = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const { model, message } = JSON.parse(line);
		const inner = {
			async *[Symbol.asyncIterator]() { yield { type: "start", partial: message }; },
			result: async () => message,
		};
		const outer = lazyStream(model, async () => inner);
		const events = [];
		for await (const event of outer) events.push(event);
		let resultSettled = false;
		void outer.result().then(() => { resultSettled = true; });
		await new Promise((resolve) => setTimeout(resolve, 25));
		process.stdout.write(`${JSON.stringify({ events, resultSettled })}\n`);
	}
}

void main().catch((error) => { console.error(error); process.exitCode = 1; });
