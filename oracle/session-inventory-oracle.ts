import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const source = pathToFileURL(process.argv[2]!);
	const { JsonlSessionRepo } = await import(source.href);
	const { NodeExecutionEnv } = await import(
		new URL("../env/nodejs.ts", source).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		const repo = new JsonlSessionRepo({
			fs: new NodeExecutionEnv({ cwd: input.root }),
			sessionsRoot: `${input.root}/sessions`,
		});
		let result: unknown;
		try {
			result = {
				ok: true,
				value: await repo.list(input.cwd === null ? {} : { cwd: input.cwd }),
			};
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
