import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Entry = { data: { nested: { value: number } } };
type View = {
	getEntry(id: string): Entry;
	getEntries(): Entry[];
	getLeafEntry(): Entry;
	getTree(): { entry: Entry; children: unknown[] }[];
	getBranch(): Entry[];
	buildContextEntries(): Entry[];
	getHeader(): { cwd: string };
	getCwd(): string;
	getSessionDir(): string;
	getSessionId(): string;
	getSessionFile(): string | undefined;
	getLeafId(): string;
	getLabel(id: string): string | undefined;
	getSessionName(): string | undefined;
	hasSessionNameState(): boolean;
	getSessionGoalJson(): string | undefined;
	getSessionGoalJsonForSession(file: string): string | undefined;
	setSessionGoalJson(goal: string): void;
	clearSessionGoalJson(): void;
	isSubagentSession(): boolean;
	getSubagentName(): string | undefined;
};
const wrapper = (await import(pathToFileURL(process.argv[2]).href)) as {
	createReadonlySessionManager(session: View): View;
};
const storage = (await import(
	new URL("./session-manager.ts", pathToFileURL(process.argv[2])).href
)) as {
	SessionManager: {
		inMemory(
			cwd: string,
		): View & { appendCustomEntry(type: string, data: unknown): string };
	};
};
for await (const _line of createInterface({ input: process.stdin })) {
	const manager = storage.SessionManager.inMemory("/private/repo");
	const view = wrapper.createReadonlySessionManager(manager);
	const id = manager.appendCustomEntry("probe", { nested: { value: 1 } });
	const snapshots = [
		view.getEntry(id),
		view.getLeafEntry(),
		...view.getEntries(),
		...view.getBranch(),
		...view.buildContextEntries(),
		view.getTree()[0].entry,
	];
	for (const entry of snapshots) entry.data.nested.value = 2;
	const header = view.getHeader();
	header.cwd = "/changed";
	view.setSessionGoalJson('{"goal":1}');
	const goal = view.getSessionGoalJson();
	const underlyingGoal = manager.getSessionGoalJson();
	const otherGoal = view.getSessionGoalJsonForSession("/other.jsonl") ?? null;
	view.clearSessionGoalJson();
	const values = {
		entryIsolated: view.getEntry(id).data.nested.value === 1,
		headerIsolated: view.getHeader().cwd === "/private/repo",
		cwd: view.getCwd(),
		sessionDir: view.getSessionDir(),
		sessionFile: view.getSessionFile() ?? null,
		leafMatches: view.getLeafId() === id,
		hasId: view.getSessionId().length > 0,
		label: view.getLabel(id) ?? null,
		name: view.getSessionName() ?? null,
		nameState: view.hasSessionNameState(),
		goal,
		underlyingGoal,
		otherGoal,
		cleared: view.getSessionGoalJson() ?? null,
		subagent: view.isSubagentSession(),
		subagentName: view.getSubagentName() ?? null,
	};
	process.stdout.write(`${JSON.stringify(values)}\n`);
}
