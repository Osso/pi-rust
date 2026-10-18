import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

// Replays a scripted producer against the real EventStream and reports what a
// consumer observes. Events are numbers; negative numbers complete the stream.

type Operation = { op: "push"; value: number } | { op: "end"; result: string | null } | { op: "fail"; message: string };

type Request = { id: number; operations: Operation[] };

type EventStreamCtor = new (
	isComplete: (event: number) => boolean,
	extractResult: (event: number) => string,
) => {
	push(event: number): void;
	end(result?: string): void;
	fail(error: unknown): void;
	[Symbol.asyncIterator](): AsyncIterator<number>;
	result(): Promise<string>;
};

async function settle(promise: Promise<string>): Promise<{ state: string; value: string | null }> {
	const pending = Symbol("pending");
	const outcome = await Promise.race([
		promise.then(
			(value) => ({ state: "resolved", value }),
			(error: unknown) => ({ state: "rejected", value: error instanceof Error ? error.message : String(error) }),
		),
		new Promise<typeof pending>((resolve) => setTimeout(() => resolve(pending), 20)),
	]);
	return outcome === pending ? { state: "pending", value: null } : outcome;
}

async function execute(request: Request, EventStream: EventStreamCtor) {
	const stream = new EventStream(
		(event) => event < 0,
		(event) => `final:${event}`,
	);
	for (const operation of request.operations) {
		if (operation.op === "push") stream.push(operation.value);
		else if (operation.op === "end") stream.end(operation.result ?? undefined);
		else stream.fail(new Error(operation.message));
	}
	const events: number[] = [];
	let failure: string | null = null;
	try {
		for await (const event of stream) events.push(event);
	} catch (error) {
		failure = error instanceof Error ? error.message : String(error);
	}
	return { id: request.id, events, failure, result: await settle(stream.result()) };
}

async function main(): Promise<void> {
	const modulePath = process.argv[2];
	if (modulePath === undefined) throw new Error("event stream oracle requires a source module path");
	const { EventStream } = (await import(pathToFileURL(modulePath).href)) as { EventStream: EventStreamCtor };
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	let chain = Promise.resolve();
	process.stdin.on("data", (chunk: Buffer) => {
		buffer += decoder.write(chunk);
		let lineFeed = buffer.indexOf("\n");
		while (lineFeed !== -1) {
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			chain = chain.then(async () => {
				const response = await execute(JSON.parse(line) as Request, EventStream);
				process.stdout.write(`${JSON.stringify(response)}\n`);
			});
			lineFeed = buffer.indexOf("\n");
		}
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`event stream oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
