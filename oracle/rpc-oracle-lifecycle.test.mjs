import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(
	await readFile(join(root, "docs/porting/source-lock.json"), "utf8"),
);

function killOwnedGroup(pid) {
	try {
		process.kill(-pid, "SIGKILL");
	} catch (error) {
		if (error.code !== "ESRCH") throw error;
	}
}

test("RPC oracle exits after child import failure while caller keeps stdin open", async () => {
	const directory = await mkdtemp(join(tmpdir(), "rpc-oracle-eof-"));
	const oracle = spawn(
		join(lock.worktree, "node_modules/.bin/tsx"),
		[
			"--tsconfig",
			join(lock.worktree, "tsconfig.json"),
			join(root, "oracle/extension-examples-j-rpc-oracle.ts"),
			join(
				lock.worktree,
				"packages/coding-agent/examples/extensions/__missing_rpc_fixture__.ts",
			),
		],
		{ detached: true, stdio: ["pipe", "pipe", "pipe"] },
	);
	let timedOut = false;
	let stderr = "";
	oracle.stderr.setEncoding("utf8");
	oracle.stderr.on("data", (chunk) => {
		stderr += chunk;
	});
	const deadline = setTimeout(() => {
		timedOut = true;
		killOwnedGroup(oracle.pid);
	}, 20_000);
	try {
		oracle.stdin.write(`${JSON.stringify({ cwd: directory, scripts: [] })}\n`);
		const [code, signal] = await once(oracle, "exit");
		assert.equal(
			timedOut,
			false,
			`oracle hung after import failure: ${stderr}`,
		);
		assert.equal(signal, null, stderr);
		assert.equal(code, 1, stderr);
		assert.match(stderr, /__missing_rpc_fixture__/);
	} finally {
		clearTimeout(deadline);
		oracle.stdin.destroy();
		killOwnedGroup(oracle.pid);
		await rm(directory, { recursive: true, force: true });
	}
});

test("RPC UI oracle rejects VM startup failure instead of waiting forever for readiness", async () => {
	const oracle = spawn(
		join(lock.worktree, "node_modules/.bin/tsx"),
		[
			"--tsconfig",
			join(lock.worktree, "tsconfig.json"),
			join(root, "oracle/example-rpc-extension-ui-oracle.ts"),
			join(lock.worktree, "packages/coding-agent/examples/rpc-extension-ui.ts"),
		],
		{ detached: true, stdio: ["pipe", "pipe", "pipe"] },
	);
	let timedOut = false;
	let stderr = "";
	oracle.stderr.setEncoding("utf8");
	oracle.stderr.on("data", (chunk) => {
		stderr += chunk;
	});
	const deadline = setTimeout(() => {
		timedOut = true;
		killOwnedGroup(oracle.pid);
	}, 20_000);
	try {
		oracle.stdin.write(
			`${JSON.stringify({ ops: [], widths: [80], terminalStartupError: "fixture terminal startup failed" })}\n`,
		);
		const [code, signal] = await once(oracle, "exit");
		assert.equal(
			timedOut,
			false,
			`VM startup failure left readiness pending: ${stderr}`,
		);
		assert.equal(signal, null, stderr);
		assert.equal(code, 1, stderr);
		assert.match(stderr, /RPC fixture exited before screen start/);
		assert.match(stderr, /fixture terminal startup failed/);
	} finally {
		clearTimeout(deadline);
		oracle.stdin.destroy();
		killOwnedGroup(oracle.pid);
	}
});
