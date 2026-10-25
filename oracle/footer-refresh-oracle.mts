import childProcess from "node:child_process";
import { mkdtemp, mkdir, writeFile, unlink, rm } from "node:fs/promises";
import { syncBuiltinESMExports } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
let branch = "main";
let executions = 0;
let hold = false;
let release: (() => void) | undefined;
Reflect.set(childProcess, "spawnSync", () => ({ status: 0, stdout: branch }));
Reflect.set(
	childProcess,
	"execFile",
	(
		_file: string,
		_args: string[],
		_options: unknown,
		callback: (error: null, stdout: string) => void,
	) => {
		executions++;
		const output = branch;
		if (hold) {
			hold = false;
			release = () => callback(null, output);
		} else setImmediate(() => callback(null, output));
	},
);
syncBuiltinESMExports();
type Provider = {
	getGitBranch(): string | null;
	setCwd(cwd: string): void;
	dispose(): void;
	onBranchChange(callback: () => void): () => void;
};
const { FooterDataProvider } = (await import(
	pathToFileURL(process.argv[2]).href
)) as { FooterDataProvider: new (cwd: string) => Provider };
const delay = (ms: number) =>
	new Promise<void>((resolve) => setTimeout(resolve, ms));
async function waitUntil(done: () => boolean) {
	const end = Date.now() + 5000;
	while (!done()) {
		if (Date.now() > end) throw new Error("private refresh fixture timeout");
		await delay(10);
	}
}
for await (const line of createInterface({ input: process.stdin })) {
	const { operation } = JSON.parse(line) as {
		operation:
			| "pending"
			| "dispose_pending"
			| "cwd_pending"
			| "unread"
			| "same"
			| "read_failure"
			| "callback_recovery";
	};
	const root = await mkdtemp(join(tmpdir(), "footer-refresh-"));
	const table = join(root, ".git/reftable");
	const list = join(table, "tables.list");
	const head = join(root, ".git/HEAD");
	await mkdir(table, { recursive: true });
	await writeFile(head, "ref: refs/heads/.invalid\n");
	await writeFile(list, "0\n");
	const pending =
		operation.endsWith("pending") || operation === "callback_recovery";
	branch = "main";
	executions = 0;
	release = undefined;
	hold = pending;
	const provider = new FooterDataProvider(root);
	const changes: (string | null)[] = [];
	let throwOnce = operation === "callback_recovery";
	provider.onBranchChange(() => {
		changes.push(provider.getGitBranch());
		if (throwOnce) {
			throwOnce = false;
			throw Object.assign(new TypeError("refresh-\ud800"), {
				code: "EREFRESH",
			});
		}
	});
	const initial = operation === "unread" ? "unread" : provider.getGitBranch();
	try {
		branch = operation === "same" ? "main" : "next";
		let direct: Promise<unknown> | undefined;
		if (operation === "callback_recovery") {
			direct = (
				Reflect.get(provider, "refreshGitBranchAsync") as () => Promise<void>
			)
				.call(provider)
				.then(
					() => null,
					(error: unknown) => {
						if (!(error instanceof Error)) throw error;
						return {
							name: error.name,
							message: error.message,
							code: Reflect.get(error, "code"),
						};
					},
				);
		} else if (operation === "read_failure") await unlink(head);
		else await writeFile(list, "1\n");
		if (pending) {
			await waitUntil(() => release !== undefined);
			await writeFile(list, "2\n");
			await writeFile(list, "3\n");
			await delay(100);
			if (operation === "dispose_pending") provider.dispose();
			if (operation === "cwd_pending") {
				const next = join(root, "other");
				await mkdir(join(next, ".git"), { recursive: true });
				await writeFile(join(next, ".git/HEAD"), "ref: refs/heads/direct\n");
				provider.setCwd(next);
			}
			branch = "last";
			const finish = release;
			if (!finish) throw new Error("missing blocked git completion");
			finish();
		}
		await delay(1700);
		const final = { branch: provider.getGitBranch(), changes, executions };
		const error = direct === undefined ? undefined : await direct;
		provider.dispose();
		process.stdout.write(
			`${JSON.stringify({ initial, final, ...(direct === undefined ? {} : { error }) })}\n`,
		);
	} finally {
		provider.dispose();
		await rm(root, { recursive: true, force: true });
	}
}
