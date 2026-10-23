import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const cwd = await import(pathToFileURL(`${root}/core/session-cwd.ts`).href);

type Issue = { sessionFile?: string; sessionCwd: string; fallbackCwd: string };
type Request = {
	operation: "nearest" | "assert" | "issue" | "format" | "reopen";
	fallback: number[];
	cwd: number[];
	file: number[] | null;
	sessionDir?: string | null;
	controlDbPath?: string;
};
const units = (text: string) =>
	Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const issueWire = (issue: Issue | undefined) =>
	issue === undefined
		? null
		: {
				sessionFile:
					issue.sessionFile === undefined ? null : units(issue.sessionFile),
				sessionCwd: units(issue.sessionCwd),
				fallbackCwd: units(issue.fallbackCwd),
			};

function runProbe(request: Request): unknown {
	const fallback = String.fromCharCode(...request.fallback);
	const manager = {
		getCwd: () => String.fromCharCode(...request.cwd),
		getSessionFile: () =>
			request.file === null ? undefined : String.fromCharCode(...request.file),
	};
	if (request.operation === "nearest")
		return units(cwd.findNearestExistingCwd(fallback));
	if (request.operation === "assert") {
		cwd.assertSessionCwdExists(manager, fallback);
		return null;
	}
	if (request.operation === "issue")
		return issueWire(cwd.getMissingSessionCwdIssue(manager, fallback));
	const issue: Issue = {
		sessionFile: manager.getSessionFile(),
		sessionCwd: manager.getCwd(),
		fallbackCwd: fallback,
	};
	if (request.operation === "format") return formatProbe(issue);
	if (request.operation === "reopen") return reopenProbe(issue, request);
	throw new Error(`Unsupported operation: ${request.operation}`);
}

function formatProbe(issue: Issue) {
	const error = new cwd.MissingSessionCwdError(issue);
	return {
		error: units(cwd.formatMissingSessionCwdError(issue)),
		prompt: units(cwd.formatMissingSessionCwdPrompt(issue)),
		name: error.name,
		message: units(error.message),
		issue: issueWire(error.issue),
	};
}

function reopenProbe(issue: Issue, request: Request) {
	if (!request.controlDbPath)
		throw new Error("controlDbPath required for reopen probe");
	const manager = cwd.reopenSessionWithCwd(
		issue,
		request.sessionDir ?? undefined,
		issue.fallbackCwd,
		request.controlDbPath,
	);
	return {
		cwd: units(manager.getCwd()),
		sessionDir: units(manager.getSessionDir()),
		controlDbPath: units(manager.getMetadataControlDbPath()),
	};
}

function errorWire(error: Error) {
	const record = error as Error & {
		code?: string;
		errno?: number;
		syscall?: string;
		path?: string;
		issue?: Issue;
	};
	return {
		name: error.name,
		message: units(error.message),
		code: record.code ?? null,
		errno: record.errno ?? null,
		syscall: record.syscall ?? null,
		path: record.path === undefined ? null : units(record.path),
		issue: issueWire(record.issue),
		typeError: error instanceof TypeError,
	};
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		console.log(JSON.stringify({ ok: true, output: runProbe(request) }));
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		console.log(JSON.stringify({ ok: false, error: errorWire(error) }));
	}
}
