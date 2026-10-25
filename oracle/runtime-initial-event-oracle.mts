import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { createAgentSessionRuntime } = await import(
	pathToFileURL(`${source}/core/agent-session-runtime.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${source}/core/session-manager.ts`).href
);
interface Request {
	cwd: string;
	event: {
		type: "session_start";
		reason: "new" | "resume";
		previousSessionFile?: string;
	} | null;
}
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		let event: unknown = "factory not called";
		let failure: unknown;
		try {
			await createAgentSessionRuntime(
				async (options: { sessionStartEvent?: unknown }) => {
					event = options.sessionStartEvent ?? null;
					throw new Error("fixture factory stop");
				},
				{
					cwd: request.cwd,
					agentDir: request.cwd,
					sessionManager: SessionManager.inMemory(request.cwd),
					...(request.event ? { sessionStartEvent: request.event } : {}),
				},
			);
		} catch (error) {
			failure =
				error instanceof Error
					? { name: error.name, message: error.message }
					: String(error);
		}
		console.log(JSON.stringify({ event, error: failure }));
	} catch (error) {
		console.log(JSON.stringify({ harnessError: String(error) }));
	}
}
