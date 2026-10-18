import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type SessionCase = {
	id: string;
	name?: string | null;
	cwd?: string;
	messageCount?: number;
	allMessagesText: string;
	modified: number;
};

type SearchCase = {
	name: string;
	sessions: SessionCase[];
	query: string;
	sortMode: "threaded" | "recent" | "relevance";
	nameFilter?: "all" | "named";
};

type SearchApi = {
	filterAndSortSessions(
		sessions: unknown[],
		query: string,
		sortMode: string,
		nameFilter?: string,
	): Array<{ id: string }>;
};

type Request = { id: RequestId; operation: "search"; cases: SearchCase[] };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function toSession(session: SessionCase): unknown {
	return {
		path: `/tmp/${session.id}.jsonl`,
		id: session.id,
		cwd: session.cwd ?? "",
		name: session.name ?? undefined,
		created: new Date(0),
		modified: new Date(session.modified),
		messageCount: session.messageCount ?? 1,
		firstMessage: "(no messages)",
		allMessagesText: session.allMessagesText,
	};
}

function runCase(api: SearchApi, searchCase: SearchCase): unknown {
	const sessions = searchCase.sessions.map(toSession);
	const result = api.filterAndSortSessions(
		sessions,
		searchCase.query,
		searchCase.sortMode,
		searchCase.nameFilter ?? "all",
	);
	return { name: searchCase.name, ids: result.map((session) => session.id) };
}

function execute(request: Request, api: SearchApi): unknown {
	return { results: request.cases.map((searchCase) => runCase(api, searchCase)) };
}

function executeRequestLine(line: string, api: SearchApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		process.stdout.write(`${JSON.stringify({ id, ok: true, output: execute(request, api) })}\n`);
	} catch (error) {
		process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
	}
}

function serveRequests(api: SearchApi): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("session search oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as SearchApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`session search oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
