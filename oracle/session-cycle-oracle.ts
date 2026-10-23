import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const { InMemorySessionStorage } = await import(
		pathToFileURL(process.argv[2]!).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		const storage = new InMemorySessionStorage({ entries: input.entries });
		console.log(JSON.stringify({ pid: process.pid }));
		await storage.getPathToRoot("a");
		process.exit(0);
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
