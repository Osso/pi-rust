import childProcess from "node:child_process";
import { EventEmitter } from "node:events";
import fs from "node:fs";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { syncBuiltinESMExports } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Callback = (error: Error | null, stdout: string) => void;
class FakeWatcher extends EventEmitter {
	active = true;
	constructor(
		readonly path: string,
		readonly listener: () => void,
	) {
		super();
	}
	close() {
		this.active = false;
	}
}
let watchers: FakeWatcher[] = [];
const systemWatch = fs.watch;
let injectRuntimeFault = false;
let branch = "main";
let executions = 0;
Reflect.set(fs, "watch", (path: string, listener: () => void) => {
	if (!injectRuntimeFault) return systemWatch(path, listener);
	const watcher = new FakeWatcher(path, listener);
	watchers.push(watcher);
	return watcher;
});
Reflect.set(childProcess, "spawnSync", () => ({ status: 0, stdout: branch }));
Reflect.set(
	childProcess,
	"execFile",
	(_file: string, _args: string[], _options: unknown, callback: Callback) => {
		executions++;
		setImmediate(() => callback(null, branch));
	},
);
syncBuiltinESMExports();
type Provider = {
	getGitBranch(): string | null;
	onBranchChange(callback: () => void): () => void;
	dispose(): void;
};
const { FooterDataProvider } = (await import(
	pathToFileURL(process.argv[2]).href
)) as { FooterDataProvider: new (cwd: string) => Provider };
const delay = (milliseconds: number) =>
	new Promise<void>((resolve) => setTimeout(resolve, milliseconds));
function emit(path: string, error = false) {
	for (const watcher of [...watchers])
		if (watcher.active && watcher.path === path) {
			if (error)
				watcher.emit("error", new Error("injected watch runtime failure"));
			else watcher.listener();
		}
}
for await (const line of createInterface({ input: process.stdin })) {
	const { operation } = JSON.parse(line) as {
		operation: "poll" | "head_fail" | "reftable_fail" | "retry";
	};
	const root = await mkdtemp(join(tmpdir(), "footer-watch-"));
	const git = join(root, ".git");
	const table = join(git, "reftable");
	const list = join(table, "tables.list");
	await mkdir(table, { recursive: true });
	await writeFile(join(git, "HEAD"), "ref: refs/heads/.invalid\n");
	await writeFile(list, "0\n");
	watchers = [];
	executions = 0;
	branch = "main";
	injectRuntimeFault = operation === "retry";
	const provider = new FooterDataProvider(root);
	const changes: (string | null)[] = [];
	provider.onBranchChange(() => changes.push(provider.getGitBranch()));
	const initial = provider.getGitBranch();
	const permissionPath =
		operation === "head_fail"
			? git
			: operation === "reftable_fail"
				? table
				: undefined;
	if (permissionPath) {
		(Reflect.get(provider, "clearGitWatchers") as () => void).call(provider);
		fs.chmodSync(permissionPath, 0o111);
		(Reflect.get(provider, "setupGitWatcher") as () => void).call(provider);
	}
	try {
		await delay(50);
		if (operation === "retry") emit(git, true);
		branch = "next";
		await writeFile(list, "1\n");
		emit(table);
		emit(list);
		await delay(operation === "poll" ? 1800 : 800);
		const before = {
			branch: provider.getGitBranch(),
			changes: [...changes],
			executions,
		};
		let after: unknown = null;
		if (operation === "retry") {
			await delay(4400);
			emit(table);
			await delay(800);
			after = {
				branch: provider.getGitBranch(),
				changes: [...changes],
				executions,
			};
		}
		provider.dispose();
		branch = "disposed";
		emit(table);
		await delay(600);
		process.stdout.write(
			`${JSON.stringify({ initial, before, after, disposed: { branch: provider.getGitBranch(), changes, executions } })}\n`,
		);
	} finally {
		provider.dispose();
		if (permissionPath) fs.chmodSync(permissionPath, 0o700);
		await rm(root, { recursive: true, force: true });
	}
}
