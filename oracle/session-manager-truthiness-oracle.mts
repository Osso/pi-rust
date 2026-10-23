import { existsSync, rmdirSync, unlinkSync } from "node:fs";
import { dirname } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const api = await import(pathToFileURL(`${root}/core/session-manager.ts`).href);
const config = await import(pathToFileURL(`${root}/config.ts`).href);
const units = (text: string) =>
	Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const optionalText = (value: number[] | null) =>
	value === null ? undefined : String.fromCharCode(...value);
type Manager = ReturnType<typeof api.SessionManager.create>;
type Request = {
	operation: string;
	agentDir: string;
	cwd: number[];
	sessionDir: number[] | null;
	controlDbPath: number[] | null;
	path: string;
	id: string;
	queryCwd: number[] | null;
	from?: number[] | null;
	cwdOverride: number[] | null;
	inspectDir?: string;
};

function readSnapshot(manager: Manager) {
	const file = manager.getSessionFile();
	return {
		cwd: units(manager.getCwd()),
		dir: units(manager.getSessionDir()),
		fileDirectory: file === undefined ? null : units(dirname(file)),
		fileExists: file !== undefined && existsSync(file),
		dirExists: existsSync(manager.getSessionDir()),
		id: manager.getSessionId(),
		entries: manager.getEntries().map((entry: { id: string }) => entry.id),
	};
}

function snapshotAndRemoveCreatedSession(manager: Manager) {
	const snapshot = readSnapshot(manager);
	const file = manager.getSessionFile();
	if (file && existsSync(file)) unlinkSync(file);
	rmdirSync(manager.getSessionDir());
	return snapshot;
}

async function runProbe(request: Request) {
	process.env[config.ENV_AGENT_DIR] = request.agentDir;
	delete process.env[config.ENV_SESSION_DIR];
	const cwd = String.fromCharCode(...request.cwd);
	const dir = optionalText(request.sessionDir);
	const database = optionalText(request.controlDbPath);
	switch (request.operation) {
		case "create":
			return snapshotAndRemoveCreatedSession(
				api.SessionManager.create(cwd, dir, { id: request.id }),
			);
		case "open":
			return readSnapshot(
				api.SessionManager.open(
					request.path,
					dir,
					optionalText(request.cwdOverride),
				),
			);
		case "readonly":
			return readSnapshot(api.SessionManager.openReadOnly(request.path, dir));
		case "continue":
			return readSnapshot(api.SessionManager.continueRecent(cwd, dir));
		case "set-file": {
			const manager = api.SessionManager.inMemory(cwd, { id: request.id });
			manager.setSessionFile(request.path);
			return readSnapshot(manager);
		}
		case "fork": {
			const from =
				request.from === undefined
					? undefined
					: request.from === null
						? null
						: String.fromCharCode(...request.from);
			return snapshotAndRemoveCreatedSession(
				api.SessionManager.forkFrom(
					request.path,
					cwd,
					dir,
					{ id: request.id },
					from,
				),
			);
		}
		case "list":
			return (await api.SessionManager.list(cwd, dir, undefined, database)).map(
				(session: { id: string }) => session.id,
			);
		case "list-all":
			return (await api.SessionManager.listAll(dir, undefined, database)).map(
				(session: { id: string }) => session.id,
			);
		case "list-archived":
			return (
				await api.SessionManager.listArchived(dir, undefined, database)
			).map((session: { id: string }) => session.id);
		case "recent": {
			const path = api.findMostRecentSession(
				request.path,
				optionalText(request.queryCwd),
			);
			return path === null ? null : units(path);
		}
		default:
			throw new Error(`Unsupported operation: ${request.operation}`);
	}
}

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		const value = await runProbe(request);
		const output =
			request.inspectDir === undefined
				? value
				: { value, directoryExists: existsSync(request.inspectDir) };
		console.log(JSON.stringify({ ok: true, output }));
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		console.log(
			JSON.stringify({
				ok: false,
				error: { name: error.name, message: units(error.message) },
			}),
		);
	}
}
