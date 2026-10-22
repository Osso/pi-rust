import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

interface Fixture {
	socketPath: string;
	snapshot: unknown;
	event: unknown;
	rejection: string;
	id: string;
	text: string;
}
interface Client {
	snapshot: unknown;
	onEvent(listener: (event: unknown) => void): () => void;
	prompt(id: string, text: string): Promise<void>;
	close(): Promise<void>;
}

async function exerciseClient(client: Client, input: Fixture, publish: (event: unknown) => void, prompts: [string, string][]) {
	try {
		const eventPromise = new Promise((resolve) => client.onEvent(resolve));
		publish(input.event);
		const event = await eventPromise;
		let error: string | undefined;
		try { await client.prompt(input.id, input.text); }
		catch (caught) {
			if (!(caught instanceof Error)) throw caught;
			error = caught.message;
		}
		return { snapshot: client.snapshot, event, error, prompts };
	} finally { await client.close(); }
}

async function main() {
	const module = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin, terminal: false })) {
		const input: Fixture = JSON.parse(line);
		const prompts: [string, string][] = [];
		let listener: ((event: unknown) => void) | undefined;
		const server = new module.ResidentConsoleServer({
			socketPath: input.socketPath,
			service: "supervisor",
			getSnapshot: () => input.snapshot,
			enqueuePrompt: (text: string, id: string) => {
				prompts.push([id, text]);
				throw new Error(input.rejection);
			},
			subscribe: (callback: (event: unknown) => void) => {
				listener = callback;
				return () => { listener = undefined; };
			},
		});
		await server.start();
		try {
			const publish = (event: unknown) => {
				if (!listener) throw new Error("Missing event subscription");
				listener(event);
			};
			const client = await module.ResidentConsoleClient.connect({ socketPath: input.socketPath, service: "supervisor" });
			const result = await exerciseClient(client, input, publish, prompts);
			process.stdout.write(`${JSON.stringify(result)}\n`);
		} finally { await server.close(); }
	}
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
