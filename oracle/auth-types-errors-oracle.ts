import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { createServer } from "node:net";

async function codexListenerOpen(): Promise<boolean> {
	return new Promise((resolve, reject) => {
		const server = createServer();
		server.once("error", (error: NodeJS.ErrnoException) => {
			if (error.code === "EADDRINUSE") resolve(true);
			else reject(error);
		});
		server.listen(1455, "127.0.0.1", () => server.close(() => resolve(false)));
	});
}

function snapshotThrown(value: unknown): unknown {
	if (value instanceof Error || value instanceof DOMException) {
		const snapshot: Record<string, unknown> = {
			name: value.name,
			message: value.message,
		};
		if ("code" in value) snapshot.code = value.code;
		return snapshot;
	}
	return value;
}

async function main(): Promise<void> {
	const source = process.argv[2];
	const anthropic = await import(pathToFileURL(source).href);
	const vertex = await import(
		pathToFileURL(join(dirname(source), "google-vertex.ts")).href
	);
	const codex = await import(
		pathToFileURL(join(dirname(source), "openai-codex.ts")).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const calls: string[] = [];
		const controller = new AbortController();
		if (request.abortUndefined) controller.abort(undefined);
		const reason = request.abortUndefined
			? controller.signal.reason
			: request.reason;
		try {
			if (request.operation === "modify") {
				const api = await import(
					pathToFileURL(join(dirname(source), "../auth/credential-store.ts"))
						.href
				);
				await new api.InMemoryCredentialStore().modify("p1", async () => {
					calls.push("modify");
					throw reason;
				});
			} else if (
				request.operation === "notify" ||
				request.operation === "notify_codex_leak"
			) {
				const provider =
					request.operation === "notify"
						? anthropic.anthropicProvider()
						: codex.openaiCodexProvider();
				await provider.auth.oauth.login({
					prompt: async (prompt: {
						type: string;
						options?: { id: string }[];
					}) => {
						if (prompt.type === "select" && prompt.options?.[0]) {
							calls.push("select");
							return prompt.options[0].id;
						}
						calls.push("prompt");
						throw "prompt reached";
					},
					notify: (event: { type: string }) => {
						calls.push(event.type);
						throw reason;
					},
				});
			} else {
				const provider =
					request.operation === "env"
						? anthropic.anthropicProvider()
						: vertex.googleVertexProvider();
				await provider.auth.apiKey.resolve({
					model: provider.getModels()[0],
					ctx: {
						env: async (name: string) => {
							calls.push(`env:${name}`);
							if (request.operation === "env") throw reason;
							return undefined;
						},
						fileExists: async (path: string) => {
							calls.push(`file:${path}`);
							throw reason;
						},
					},
				});
			}
			process.stdout.write(`${JSON.stringify({ ok: true, calls })}\n`);
		} catch (error) {
			const listenerOpen =
				request.operation === "notify_codex_leak"
					? await codexListenerOpen()
					: undefined;
			process.stdout.write(
				`${JSON.stringify({ ok: false, thrown: snapshotThrown(error), calls, listenerOpen })}\n`,
			);
		}
	}
}
void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
