import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const source = pathToFileURL(process.argv[2]!);
	const branch = await import(source.href);
	const { Session } = await import(
		new URL("../session/session.ts", source).href
	);
	const { InMemorySessionStorage } = await import(
		new URL("../session/memory-storage.ts", source).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		let result: unknown;
		try {
			const storage = new InMemorySessionStorage({ entries: input.entries });
			if (input.missingLookup) storage.getEntry = async () => undefined;
			const value = await branch.collectEntriesForBranchSummary(
				new Session(storage),
				input.old,
				input.target,
			);
			result = { ok: true, value };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = {
				ok: false,
				error: {
					name: error.name,
					code: (error as Error & { code?: string }).code,
					message: error.message,
				},
			};
		}
		console.log(JSON.stringify(result));
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
