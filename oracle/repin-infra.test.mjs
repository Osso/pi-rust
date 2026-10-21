import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	mkdtempSync,
	mkdirSync,
	writeFileSync,
	existsSync,
	rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = fileURLToPath(new URL("../", import.meta.url));
const helper = join(root, "oracle/typecheck-reference.mjs");

for (const action of ["setup", "check"]) {
	test(`${action} rejects blocked reference without preparing source or reference`, () => {
		const directory = mkdtempSync(join(tmpdir(), "pi-repin-blocked-"));
		try {
			mkdirSync(join(directory, "docs/porting"), { recursive: true });
			const worktree = join(directory, "reference");
			writeFileSync(
				join(directory, "docs/porting/source-lock.json"),
				JSON.stringify({
					worktree: join(directory, "source"),
					commit: "a".repeat(40),
					typecheckReference: {
						status: "blocked",
						worktree,
						todo: "old patch does not apply",
					},
				}),
			);
			const result = spawnSync(process.execPath, [helper, action, directory], {
				encoding: "utf8",
			});
			assert.equal(result.status, 1, result.stdout + result.stderr);
			assert.match(result.stderr, /blocked.*old patch does not apply/);
			assert.match(result.stderr, /oracle-gate.md/);
			assert.equal(existsSync(worktree), false);
			assert.equal(existsSync(join(directory, "source")), false);
		} finally {
			rmSync(directory, { recursive: true, force: true });
		}
	});
}

test("source type imports resolve through the lock-selected package tree", async () => {
	const { buildSourceTsconfig } = await import("./source-tsconfig.mjs");
	const directory = mkdtempSync(join(tmpdir(), "pi-repin-types-"));
	try {
		mkdirSync(join(directory, "packages/a/src"), { recursive: true });
		writeFileSync(
			join(directory, "tsconfig.base.json"),
			JSON.stringify({ compilerOptions: { strict: true } }),
		);
		writeFileSync(
			join(directory, "tsconfig.json"),
			JSON.stringify({ compilerOptions: {} }),
		);
		writeFileSync(
			join(directory, "packages/a/src/value.ts"),
			"export const value: number = 3;",
		);
		const fixture = join(directory, "fixture.ts");
		writeFileSync(
			fixture,
			'import { value } from "@pi-oracle-source/a/src/value.ts"; const n: number = value; void n;',
		);
		const config = buildSourceTsconfig(directory, [fixture]);
		config.compilerOptions.allowImportingTsExtensions = true;
		config.compilerOptions.types = [];
		config.compilerOptions.moduleResolution = "bundler";
		config.compilerOptions.module = "esnext";
		const configPath = join(directory, "tsconfig.proof.json");
		writeFileSync(configPath, JSON.stringify(config));
		const lock = JSON.parse(
			(await import("node:fs")).readFileSync(
				join(root, "docs/porting/source-lock.json"),
				"utf8",
			),
		);
		const compiler = resolve(lock.worktree, "node_modules/.bin/tsgo");
		const result = spawnSync(compiler, ["-p", configPath], {
			encoding: "utf8",
		});
		assert.equal(result.status, 0, result.stdout + result.stderr);
		writeFileSync(
			join(directory, "packages/a/src/value.ts"),
			'export const value: string = "bad";',
		);
		const mutation = spawnSync(compiler, ["-p", configPath], {
			encoding: "utf8",
		});
		assert.equal(mutation.status, 2, mutation.stdout + mutation.stderr);
		assert.match(mutation.stdout + mutation.stderr, /TS2322/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});
