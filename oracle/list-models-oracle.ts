import { pathToFileURL } from "node:url";
import { serve } from "./config-auth-oracle-support.ts";

type Event = { stream: "stdout" | "stderr"; text: string };

function describeFailure(error: unknown): unknown {
	if (!(error instanceof Error)) throw new Error("Fixture must throw an Error");
	return { name: error.name, message: error.message, constructor: error.constructor.name, properties: Object.fromEntries(Object.entries(error)) };
}

async function captureStreams(action: () => Promise<void>) {
	const originalStdout = process.stdout.write.bind(process.stdout);
	const originalStderr = process.stderr.write.bind(process.stderr);
	const events: Event[] = [];
	let failure: unknown = null;
	const record = (stream: Event["stream"], chunk: string | Uint8Array) => {
		events.push({ stream, text: typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8") });
		return true;
	};
	process.stdout.write = ((chunk: string | Uint8Array) => record("stdout", chunk)) as typeof process.stdout.write;
	process.stderr.write = ((chunk: string | Uint8Array) => record("stderr", chunk)) as typeof process.stderr.write;
	try { await action(); }
	catch (error) { failure = describeFailure(error); }
	finally { process.stdout.write = originalStdout; process.stderr.write = originalStderr; }
	const stdout = events.filter(event => event.stream === "stdout").map(event => event.text).join("");
	const stderr = events.filter(event => event.stream === "stderr").map(event => event.text).join("");
	return { warning: stderr.length > 0 ? stderr.replace(/\n$/, "") : null, stdout, events, failure };
}

async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]).href);
	serve((request) => captureStreams(() => api.listModels({
		getError: () => request.loadError,
		getAvailable: () => {
			if (request.availableError) {
				const constructors = { Error, TypeError, RangeError };
				const Constructor = constructors[request.availableError.name as keyof typeof constructors];
				if (!Constructor) throw new Error("Unsupported fixture error constructor");
				const error = new Constructor(request.availableError.message);
				if (request.availableError.code !== undefined) Object.assign(error, { code: request.availableError.code });
				throw error;
			}
			return request.models;
		},
	}, request.searchPattern)));
}
void main();
