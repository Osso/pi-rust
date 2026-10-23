import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("compat source module required");
const api = await import(pathToFileURL(source).href);

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { operation: string; model: unknown };
	for (const key of ["HOME", "GOOGLE_APPLICATION_CREDENTIALS", "GOOGLE_CLOUD_API_KEY"]) {
		delete process.env[key];
	}
	process.env.PI_TEST_FAIL_PASSWD = "1";
	try {
		const context = { messages: [] };
		if (request.operation === "stream") api.stream(request.model, context, {});
		else if (request.operation === "streamSimple") api.streamSimple(request.model, context, {});
		else throw new Error(`Unexpected operation: ${request.operation}`);
		console.log(JSON.stringify({ threw: false }));
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		console.log(JSON.stringify({ threw: true, message: error.message, error: String(error) }));
	}
}
