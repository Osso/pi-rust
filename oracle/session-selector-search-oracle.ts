// Differential oracle for `modes/interactive/components/session-selector-search.ts`.
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Api = Record<string, any>;
type Request = { id: number; operation: string; [key: string]: any };

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error ? { name: error.name, message: error.message } : { name: "Unknown", message: String(error) };
}

function sessionOf(value: any): any {
	return {
		path: `/tmp/${value.id}.jsonl`,
		id: value.id,
		cwd: value.cwd ?? "",
		name: value.name ?? undefined,
		created: new Date(value.createdMs ?? 0),
		modified: new Date(value.modifiedMs ?? 0),
		messageCount: value.messageCount ?? 1,
		firstMessage: value.firstMessage ?? "(no messages)",
		allMessagesText: value.allMessagesText ?? "",
	};
}

const handlers: Record<string, (api: Api, request: Request) => unknown> = {
	filter: (api, request) =>
		api
			.filterAndSortSessions(
				request.sessions.map(sessionOf),
				request.query,
				request.sortMode,
				request.nameFilter ?? "all",
			)
			.map((session: any) => session.id),
	parse: (api, request) =>
		request.queries.map((query: string) => {
			const parsed = api.parseSearchQuery(query);
			return {
				mode: parsed.mode,
				tokens: parsed.tokens,
				hasRegex: parsed.regex !== null,
				errorPresent: parsed.error !== undefined,
			};
		}),
	match: (api, request) => {
		const parsed = api.parseSearchQuery(request.query);
		return request.sessions.map(sessionOf).map((session: any) => api.matchSession(session, parsed));
	},
};

async function main(): Promise<void> {
	const sourceRoot = process.argv[2];
	if (!sourceRoot) throw new Error("session-selector-search oracle requires the coding-agent src directory");
	const api = (await import(
		pathToFileURL(`${sourceRoot}/modes/interactive/components/session-selector-search.ts`).href
	)) as Api;
	for await (const line of createInterface({ input: process.stdin })) {
		let id: number | null = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const handler = handlers[request.operation];
			if (!handler) throw new Error(`unknown operation ${request.operation}`);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output: handler(api, request) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ id, ok: false, error: errorOf(error) })}\n`);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`session-selector-search oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
