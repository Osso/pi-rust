import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { selectTypecheckReference } from "./typecheck-reference.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const { worktree } = selectTypecheckReference(repoRoot);
const compiler = join(worktree, "node_modules/.bin/tsgo");
const reporter = join(repoRoot, "oracle/typecheck-report.mjs");

function runCompiler(command, args) {
	return spawnSync(process.execPath, [reporter, command, ...args], {
		cwd: repoRoot,
		input: "",
		encoding: "utf8",
	});
}

function compileFixture(t, fixture) {
	const directory = mkdtempSync(join(tmpdir(), ".typecheck-config-"));
	t.after(() => rmSync(directory, { recursive: true, force: true }));
	const config = spawnSync(
		process.execPath,
		[
			join(repoRoot, "oracle/oracle-tsconfig.mjs"),
			dirname(fixture),
			basename(fixture),
		],
		{ encoding: "utf8" },
	);
	assert.equal(config.status, 0, config.stderr);
	const configPath = join(directory, "tsconfig.json");
	writeFileSync(configPath, config.stdout);
	return { fixture, result: runCompiler(compiler, ["-p", configPath]) };
}

function checkFixture(t, source) {
	const directory = mkdtempSync(join(tmpdir(), ".typecheck-regression-"));
	t.after(() => rmSync(directory, { recursive: true, force: true }));
	const fixture = join(directory, "fixture.ts");
	writeFileSync(fixture, source);
	return compileFixture(t, fixture);
}

test("invalid repository oracle reports its type error and fails", (t) => {
	const { fixture, result } = compileFixture(
		t,
		join(repoRoot, "oracle/fixtures/invalid-typecheck.ts"),
	);
	assert.notEqual(result.status, 0);
	assert.match(result.stdout + result.stderr, /TS2322/);
	assert.ok(
		(result.stdout + result.stderr).includes(relative(repoRoot, fixture)),
	);
});

test("invalid temporary fixture reports its type error and fails", (t) => {
	const { fixture, result } = checkFixture(
		t,
		'const value: number = "invalid";',
	);
	assert.notEqual(result.status, 0);
	assert.match(result.stdout + result.stderr, /TS2322/);
	assert.ok((result.stdout + result.stderr).includes(fixture));
});

test("valid temporary fixture passes", (t) => {
	const { result } = checkFixture(t, "const value: number = 42;");
	assert.equal(result.status, 0, result.stdout + result.stderr);
	assert.equal(result.stdout + result.stderr, "");
});

test("no-emit fixtures accept absolute TypeScript module imports", (t) => {
	const modulePath = join(
		worktree,
		"packages/coding-agent/src/core/skill-block.ts",
	);
	const source = `import { parseSkillBlock } from ${JSON.stringify(modulePath)};
const parsed = parseSkillBlock("plain user message");
const absent: null = parsed === null ? parsed : null;
void absent;`;
	const { result } = checkFixture(t, source);
	assert.equal(result.status, 0, result.stdout + result.stderr);
});

test("compiler exit status survives without TypeScript diagnostics", () => {
	const result = runCompiler(process.execPath, ["-e", "process.exit(42)"]);
	assert.equal(result.status, 42);
});

test("compiler crash fails with its signal reported", () => {
	const result = runCompiler(process.execPath, [
		"-e",
		'process.kill(process.pid, "SIGKILL")',
	]);
	assert.notEqual(result.status, 0);
	assert.match(result.stderr, /SIGKILL/);
});

test("compiler launch failure fails with context", () => {
	const result = runCompiler("/nonexistent/pi-oracle-compiler", []);
	assert.notEqual(result.status, 0);
	assert.match(result.stderr, /ENOENT/);
});

test("compiler stdout and stderr remain complete and separate", () => {
	const result = runCompiler(process.execPath, [
		"-e",
		'process.stdout.write("context\\n  detail\\n"); process.stderr.write("fatal: compiler failure\\n"); process.exit(2);',
	]);
	assert.equal(result.status, 2);
	assert.equal(result.stdout, "context\n  detail\n");
	assert.equal(result.stderr, "fatal: compiler failure\n");
});

test("compiler config diagnostics without file spans fail unchanged", () => {
	const result = runCompiler(compiler, ["--invalid-oracle-option"]);
	assert.notEqual(result.status, 0);
	assert.match(result.stdout + result.stderr, /Unknown compiler option/);
});
