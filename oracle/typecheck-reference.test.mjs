import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	mkdtempSync,
	mkdirSync,
	readFileSync,
	writeFileSync,
	rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const helper = join(root, "oracle/typecheck-reference.mjs");
const lock = JSON.parse(
	readFileSync(join(root, "docs/porting/source-lock.json"), "utf8"),
);
function command(program, args, cwd = root) {
	const result = spawnSync(program, args, { cwd, encoding: "utf8" });
	assert.ifError(result.error);
	return { ...result, output: result.stdout + result.stderr };
}

test("unmodified new pin is a valid type reference with no clone or patch", () => {
	const result = command(process.execPath, [helper, "check"]);
	assert.equal(result.status, 0, result.output);
	assert.equal(result.stdout.trim(), lock.worktree);
});

test("unmodified reference rejects wrong HEAD, dirty catalog and missing compiler", () => {
	const directory = mkdtempSync(join(tmpdir(), "pi-unmodified-reference-"));
	try {
		const source = join(directory, "source");
		mkdirSync(join(source, "packages/ai/src"), { recursive: true });
		mkdirSync(join(source, "node_modules/.bin"), { recursive: true });
		mkdirSync(join(directory, "docs/porting"), { recursive: true });
		writeFileSync(
			join(source, "packages/ai/src/models.generated.ts"),
			"export {};\n",
		);
		writeFileSync(join(source, "tsconfig.json"), "{}");
		writeFileSync(join(source, "node_modules/.bin/tsgo"), "fixture");
		writeFileSync(join(source, ".gitignore"), "node_modules/\n");
		for (const args of [
			["init", "-q"],
			["add", "."],
			[
				"-c",
				"user.name=Fixture",
				"-c",
				"user.email=fixture@example.invalid",
				"-c",
				"commit.gpgsign=false",
				"commit",
				"-qm",
				"source",
			],
		]) {
			const result = command("git", args, source);
			assert.equal(result.status, 0, result.output);
		}
		const commit = command("git", ["rev-parse", "HEAD"], source).stdout.trim();
		const configured = {
			worktree: source,
			commit,
			modelCatalog: { path: "packages/ai/src/models.generated.ts" },
			typecheckReference: { status: "unmodified", baseCommit: commit },
		};
		const save = () =>
			writeFileSync(
				join(directory, "docs/porting/source-lock.json"),
				JSON.stringify(configured),
			);
		const check = () => command(process.execPath, [helper, "check", directory]);
		save();
		assert.equal(check().status, 0, check().output);
		configured.commit = "0".repeat(40);
		configured.typecheckReference.baseCommit = configured.commit;
		save();
		assert.match(check().output, /HEAD mismatch/);
		configured.commit = commit;
		configured.typecheckReference.baseCommit = commit;
		save();
		writeFileSync(
			join(source, "packages/ai/src/models.generated.ts"),
			"export const dirty = true;\n",
		);
		assert.match(check().output, /dirty/);
		writeFileSync(
			join(source, "packages/ai/src/models.generated.ts"),
			"export {};\n",
		);
		rmSync(join(source, "node_modules/.bin/tsgo"));
		assert.match(check().output, /missing.*tsgo/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("new pin public index compiles without provider patch; invalid fixture remains rejected", () => {
	const directory = mkdtempSync(join(tmpdir(), "pi-unmodified-compiler-"));
	try {
		const configPath = join(directory, "tsconfig.json");
		const index = join(lock.worktree, "packages/coding-agent/src/index.ts");
		const compiler = join(lock.worktree, "node_modules/.bin/tsgo");
		writeFileSync(
			configPath,
			JSON.stringify(buildSourceTsconfig(lock.worktree, [index])),
		);
		const valid = command(compiler, ["-p", configPath]);
		assert.equal(valid.status, 0, valid.output);
		const invalid = join(directory, "invalid.ts");
		writeFileSync(invalid, "const bad: number = 'not a number'; void bad;\n");
		writeFileSync(
			configPath,
			JSON.stringify(buildSourceTsconfig(lock.worktree, [invalid])),
		);
		const rejected = command(compiler, ["-p", configPath]);
		assert.equal(rejected.status, 2, rejected.output);
		assert.match(rejected.output, /TS2322/);
		assert.equal(
			(rejected.output.match(/error TS/g) ?? []).length,
			1,
			rejected.output,
		);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});
