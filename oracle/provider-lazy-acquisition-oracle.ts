import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

function createRejection(kind: unknown, value: unknown): unknown {
	if (kind === "error")
		return Object.assign(new TypeError(value as string), { code: "E_LAZY" });
	if (kind === "conversion") return { toString: null, valueOf: null };
	return value;
}

async function main(): Promise<void> {
	const { lazyApi } = await import(pathToFileURL(process.argv[2]).href);
	serve(async (request) => {
		let callbackCount = 0;
		const rejection = createRejection(request.kind, request.value);
		const fail = () => {
			callbackCount++;
			throw rejection;
		};
		const acquire = request.inner
			? () => ({ async *[Symbol.asyncIterator]() { throw rejection; } })
			: fail;
		const api = lazyApi(async () => ({ stream: acquire, streamSimple: acquire }));
		const before = Date.now();
		const stream =
			request.method === "stream"
				? api.stream(request.model, { messages: [] })
				: api.streamSimple(request.model, { messages: [] });
		const events: unknown[] = [];
		for await (const event of stream) events.push(event);
		const result = await stream.result();
		return {
			clock: { before, after: Date.now() },
			snapshot: { callbackCount, events, result },
		};
	});
}
main().catch((error) => {
	console.error(error);
	process.exit(1);
});
