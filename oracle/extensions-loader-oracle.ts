import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const loader = await import(pathToFileURL(process.argv[2]).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		try {
			if (request.operation === "unbound") {
				const runtime = loader.createExtensionRuntime();
				const errors: Array<string | null> = [];
				for (const action of [
					"requestResumeContinuation",
					"callTool",
					"callCommand",
					"setModel",
				]) {
					try {
						await runtime[action]();
						errors.push(null);
					} catch (error) {
						errors.push(error instanceof Error ? error.message : String(error));
					}
				}
				process.stdout.write(`${JSON.stringify({ id: request.id, errors })}\n`);
				continue;
			}
			const result = request.discover
				? await loader.discoverAndLoadExtensions(
						request.paths,
						request.cwd,
						request.agentDir,
					)
				: await loader.loadExtensions(request.paths, request.cwd);
			process.stdout.write(
				`${JSON.stringify({ id: request.id, ok: true, extensions: result.extensions.map((extension: { path: string; resolvedPath: string; sourceInfo: unknown }) => ({ path: extension.path, resolvedPath: extension.resolvedPath, sourceInfo: extension.sourceInfo })), errors: result.errors })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ id: request.id, ok: false, message: error instanceof Error ? error.message : String(error) })}\n`,
			);
		}
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
