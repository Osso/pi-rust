import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { AgentSessionRuntime } = await import(
	pathToFileURL(`${root}/core/agent-session-runtime.ts`).href
);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const { path } = JSON.parse(line) as { path: string };
		const runtime = Object.assign(
			Object.create(AgentSessionRuntime.prototype),
			{
				_session: { noSupervisor: false },
				lifecycleTransitionTail: Promise.resolve(),
			},
		);
		try {
			await runtime.importFromJsonl(path);
			throw new Error("missing import unexpectedly succeeded");
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			console.log(
				JSON.stringify({
					ok: true,
					output: {
						name: error.name,
						message: error.message,
						...("filePath" in error ? { filePath: error.filePath } : {}),
					},
				}),
			);
		}
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
