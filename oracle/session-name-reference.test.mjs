import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const setupDoc = "docs/porting/session-name-oracle.md";

function fixture(t, state) {
	const directory = mkdtempSync(join(tmpdir(), "pi-v15-preflight-"));
	t.after(() => rmSync(directory, { recursive: true, force: true }));
	mkdirSync(join(directory, "oracle"));
	mkdirSync(join(directory, "docs/porting"), { recursive: true });
	copyFileSync(join(root, "run-tests.sh"), join(directory, "run-tests.sh"));
	const helper = "oracle/session-name-reference.mjs";
	copyFileSync(join(root, helper), join(directory, helper));
	const reference = join(directory, "target/session-name-oracle");
	let commit = "0".repeat(40);
	if (state !== "missing") {
		mkdirSync(reference, { recursive: true });
		for (const args of [
			["init", "--quiet"],
			[
				"-c",
				"user.name=Fixture",
				"-c",
				"user.email=fixture@example.invalid",
				"-c",
				"commit.gpgsign=false",
				"commit",
				"--quiet",
				"--allow-empty",
				"-m",
				"fixture",
			],
		]) {
			const result = spawnSync("git", args, {
				cwd: reference,
				encoding: "utf8",
			});
			assert.equal(result.status, 0, result.stderr);
		}
		if (state !== "wrong-head") {
			commit = spawnSync("git", ["rev-parse", "HEAD"], {
				cwd: reference,
				encoding: "utf8",
			}).stdout.trim();
		}
		if (state !== "missing-environment") {
			mkdirSync(join(reference, "node_modules/.bin"), { recursive: true });
			mkdirSync(join(reference, "packages/ai/src"), {
				recursive: true,
			});
			writeFileSync(join(reference, "node_modules/.bin/tsx"), "fixture");
			writeFileSync(join(reference, "tsconfig.json"), "{}");
			if (state !== "missing-catalog")
				writeFileSync(
					join(reference, "packages/ai/src/models.generated.ts"),
					"export {};",
				);
		}
	}
	writeFileSync(
		join(directory, "docs/porting/source-lock.json"),
		JSON.stringify({ worktree: "target/session-name-oracle", commit }),
	);
	const bin = join(directory, "bin");
	mkdirSync(bin);
	writeFileSync(
		join(bin, "cargo"),
		"#!/usr/bin/env node\nrequire('node:fs').writeFileSync(process.env.CARGO_MARKER, 'called'); process.exit(73);\n",
		{ mode: 0o755 },
	);
	return { directory, bin, marker: join(directory, "cargo-called") };
}

for (const mode of ["all", "differential"]) {
	for (const state of ["missing", "wrong-head"]) {
		test(`${mode} rejects ${state} v15 reference before cargo`, (t) => {
			const f = fixture(t, state);
			const result = spawnSync(join(f.directory, "run-tests.sh"), [mode], {
				cwd: f.directory,
				encoding: "utf8",
				timeout: 10000,
				env: {
					...process.env,
					PATH: `${f.bin}:${process.env.PATH}`,
					CARGO_MARKER: f.marker,
				},
			});
			assert.equal(result.status, 1, result.stdout + result.stderr);
			assert.ok(result.stderr.includes(setupDoc), result.stderr);
			assert.match(
				result.stderr,
				state === "missing"
					? /missing v15 reference/
					: /v15 reference HEAD mismatch/,
			);
			assert.equal(existsSync(f.marker), false, "cargo must not start");
		});
	}
}

for (const state of ["valid", "missing-environment", "missing-catalog"]) {
	test(`explicit v15 preflight: ${state}`, (t) => {
		const f = fixture(t, state);
		const lockBefore = readFileSync(
			join(f.directory, "docs/porting/source-lock.json"),
			"utf8",
		);
		const result = spawnSync(
			process.execPath,
			[join(f.directory, "oracle/session-name-reference.mjs"), f.directory],
			{ encoding: "utf8", timeout: 10000 },
		);
		assert.equal(
			result.status,
			state === "valid" ? 0 : 1,
			result.stdout + result.stderr,
		);
		if (state === "valid")
			assert.equal(
				result.stdout.trim(),
				join(f.directory, "target/session-name-oracle"),
			);
		else {
			assert.ok(result.stderr.includes(setupDoc), result.stderr);
			if (state === "missing-catalog")
				assert.match(result.stderr, /models\.generated\.ts/);
		}
		assert.equal(
			readFileSync(join(f.directory, "docs/porting/source-lock.json"), "utf8"),
			lockBefore,
		);
	});
}
