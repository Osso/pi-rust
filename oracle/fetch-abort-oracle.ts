// Native fetch rejection boundary used by the pinned provider/OAuth modules.
import { createServer } from "node:http";
import { serve } from "./config-auth-oracle-support.ts";
import { describeAbortReason } from "./abort-reason-snapshot.ts";

function reason(input: Record<string, unknown>): unknown {
	if (input.kind === "default") return undefined;
	if (input.kind === "number") return Number(input.value);
	if (input.kind === "error") return Object.assign(new Error(input.message as string), { name: input.name ?? "Error", ...input.properties as object });
	if (input.kind === "dom") return new DOMException(input.message as string, input.name as string);
	return input.value;
}

async function observe(input: Record<string, unknown>): Promise<unknown> {
	const controller = new AbortController();
	const server = createServer((_request, response) => {
		if (input.phase === "body") { response.writeHead(200); response.flushHeaders(); }
		else controller.abort(reason(input));
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	const address = server.address();
	if (!address || typeof address === "string") throw new Error("missing fixture address");
	try {
		if (input.phase === "pre") controller.abort(reason(input));
		const response = await fetch(`http://127.0.0.1:${address.port}`, { signal: controller.signal });
		controller.abort(reason(input));
		await response.body!.getReader().read();
		throw new Error("fixture unexpectedly succeeded");
	} catch (error) {
		return { rejection: describeAbortReason(error), signal: describeAbortReason(controller.signal.reason) };
	} finally {
		server.closeAllConnections();
		await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
	}
}
serve(observe);
