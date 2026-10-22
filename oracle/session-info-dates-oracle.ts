import { readFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
const root = process.argv[2];
const moduleAt = (relative: string) => import(pathToFileURL(`${root}/${relative}`).href);
const ts = await import(pathToFileURL(`${root}/../../../node_modules/typescript/lib/typescript.js`).href);

function loadFunction(relative: string, name: string, bindings: Record<string, unknown> = {}): (...args: any[]) => any {
	const text = readFileSync(`${root}/${relative}`, "utf8");
	const source = ts.createSourceFile(relative, text, ts.ScriptTarget.Latest, true);
	const declaration = source.statements.find((node: any) => ts.isFunctionDeclaration(node) && node.name?.text === name);
	if (!declaration) throw new Error(`Missing frozen function ${relative}:${name}`);
	const output = ts.transpileModule(declaration.getText(source), { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
	return new Function(...Object.keys(bindings), `${output}\nreturn ${name};`)(...Object.values(bindings));
}

function iso(date: Date): unknown {
	try { return { ok: true, value: date.toISOString() }; }
	catch (error) { return { ok: false, name: (error as Error).name, message: (error as Error).message }; }
}
function info(value: any): unknown {
	if (value === null) return null;
	return { id: value.id, created: value.created.getTime(), modified: value.modified.getTime(), createdIso: iso(value.created), modifiedIso: iso(value.modified) };
}
function sessions(rows: any[]): any[] {
	return rows.map(row => ({ path: `/tmp/${row.id}.jsonl`, cwd: "/work", firstMessage: "chair", messageCount: 1, ...row, created: new Date(Number(row.created)), modified: new Date(Number(row.modified)) }));
}

async function execute(request: any): Promise<unknown> {
	if (request.operation === "build") {
		const api = await moduleAt("core/session-manager.ts");
		return info(await api.buildSessionInfo(request.path));
	}
	if (request.operation === "list") {
		const api = await moduleAt("core/session-manager.ts");
		return (await api.SessionManager.listAll(request.dir, undefined, request.db)).map(info);
	}
	if (request.operation === "appendMetadata") {
		const api = await moduleAt("core/session-manager.ts");
		const control = await moduleAt("core/session-control-db.ts");
		const manager = api.SessionManager.create("/work", request.dir);
		manager.setMetadataControlDbPath(request.db);
		let outcome: unknown;
		try { manager.appendMessage({ role: "user", content: "chair", timestamp: Number(request.timestamp) }); outcome = { ok: true }; }
		catch (error) { outcome = { ok: false, name: (error as Error).name, message: (error as Error).message }; }
		const metadata = control.readSessionMetadata(request.db, manager.getSessionFile());
		return { outcome, entries: manager.getEntries().length, messageCount: metadata.messageCount, firstMessage: metadata.firstMessage, modified: metadata.modifiedAt === metadata.createdAt ? "<header>" : metadata.modifiedAt };
	}
	if (request.operation === "picker") {
		const resident = await moduleAt("core/resident-session.ts");
		const load = loadFunction("cli/session-picker.ts", "loadSessions", { isResidentSession: resident.isResidentSession });
		return (await load(async () => sessions(request.rows), undefined, request.archived)).map((session: any) => session.id);
	}
	if (request.operation === "commandMetadata") {
		const writable = loadFunction("cli/sessions-command.ts", "writableMetadata");
		return sessions(request.rows).map(session => {
			try { return { ok: true, value: writable(session) }; }
			catch (error) { return { ok: false, name: (error as Error).name, message: (error as Error).message }; }
		});
	}
	if (request.operation === "search") {
		const api = await moduleAt("modes/interactive/components/session-selector-search.ts");
		return ["recent", "relevance", "threaded"].map(sort => api.filterAndSortSessions(sessions(request.rows), request.query, sort).map((session: any) => session.id));
	}
	if (request.operation === "selector") {
		const theme = await moduleAt("modes/interactive/theme/theme.ts");
		theme.initTheme("dark", false);
		const api = await moduleAt("modes/interactive/components/session-selector.ts");
		const pending = () => new Promise(() => {});
		const selector = new api.SessionSelectorComponent(pending, pending, () => {}, () => {}, () => {}, () => {});
		const list = selector.getSessionList();
		list.setSessions(sessions(request.rows), false);
		list.setSortMode(request.sort);
		return { frame: list.render(request.width), selected: list.getSelectedSessionPath() ?? null };
	}
	throw new Error(`Unknown Date operation ${request.operation}`);
}

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	try { process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, output: await execute(request) })}\n`); }
	catch (error) { process.stdout.write(`${JSON.stringify({ id: request.id, ok: false, name: (error as Error).name, message: (error as Error).message })}\n`); }
}
}
void main().catch(error => { console.error(error); process.exitCode = 1; });
