import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import {
	mkdtempSync,
	readFileSync,
	writeFileSync,
	mkdirSync,
	cpSync,
	rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const lock = JSON.parse(
	readFileSync(join(root, "docs/porting/source-lock.json"), "utf8"),
);
const helper = join(root, "oracle/typecheck-reference.mjs");
function command(program, args, cwd = root) {
	const result = spawnSync(program, args, { cwd, encoding: "utf8" });
	assert.ifError(result.error);
	return { ...result, output: result.stdout + result.stderr };
}
function success(result) {
	assert.equal(result.status, 0, result.output);
}

// Real compiler boundary: preserve the original two failures, then compile exactly
// the same package index against a disposable corrected reference.
test("original errors, corrected reference, emitted bytes, and reference rejection boundaries", () => {
	const directory = mkdtempSync(join(tmpdir(), "pi-type-reference-test-"));
	try {
		const fixtureRoot = join(directory, "repo");
		const docs = join(fixtureRoot, "docs/porting");
		mkdirSync(docs, { recursive: true });
		const configured = structuredClone(lock);
		assert.ok(
			configured.typecheckReference,
			"missing separately pinned typecheckReference metadata",
		);
		configured.typecheckReference.worktree = join(directory, "reference");
		cpSync(
			join(root, configured.typecheckReference.patch),
			join(fixtureRoot, configured.typecheckReference.patch),
			{ recursive: true },
		);
		writeFileSync(join(docs, "source-lock.json"), JSON.stringify(configured));
		const check = () =>
			command(process.execPath, [helper, "check", fixtureRoot]);
		assert.match(check().output, /setup/);
		success(command(process.execPath, [helper, "setup", fixtureRoot]));
		success(check());
		const reference = configured.typecheckReference.worktree;
		configured.worktree = reference;
		writeFileSync(join(docs, "source-lock.json"), JSON.stringify(configured));
		assert.match(check().output, /original source HEAD mismatch/);
		configured.worktree = lock.worktree;
		const approvedTree = configured.typecheckReference.tree;
		configured.typecheckReference.tree = "0".repeat(40);
		writeFileSync(join(docs, "source-lock.json"), JSON.stringify(configured));
		assert.match(check().output, /tree mismatch/);
		configured.typecheckReference.tree = approvedTree;
		const approvedHash = configured.typecheckReference.patchSha256;
		configured.typecheckReference.patchSha256 = "0".repeat(64);
		writeFileSync(join(docs, "source-lock.json"), JSON.stringify(configured));
		assert.match(check().output, /patch hash mismatch/);
		configured.typecheckReference.patchSha256 = approvedHash;
		writeFileSync(join(docs, "source-lock.json"), JSON.stringify(configured));
		const compiler = join(lock.worktree, "node_modules/.bin/tsgo");
		function compile(source) {
			const fixture = join(directory, "fixture.ts");
			writeFileSync(
				fixture,
				`import * as api from ${JSON.stringify(join(source, "packages/coding-agent/src/index.ts"))};\nvoid api;\n`,
			);
			const config = join(directory, "tsconfig.json");
			writeFileSync(
				config,
				JSON.stringify(
					buildSourceTsconfig(source, [
						fixture,
						join(
							source,
							"packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
						),
					]),
				),
			);
			return command(compiler, ["-p", config]);
		}
		const original = compile(lock.worktree);
		assert.equal(original.status, 2, original.output);
		assert.match(
			original.output,
			/cloudflare-ai-gateway.ts\(19,4\): error TS2353/,
		);
		assert.match(original.output, /opencode-go.ts\(13,3\): error TS2322/);
		assert.equal(
			(original.output.match(/error TS/g) ?? []).length,
			2,
			original.output,
		);
		success(compile(reference));
		const require = createRequire(import.meta.url);
		const ts = require(
			join(lock.worktree, "node_modules/typescript/lib/typescript.js"),
		);
		for (const name of ["cloudflare-ai-gateway.ts", "opencode-go.ts"]) {
			const relative = `packages/ai/src/providers/${name}`;
			const emit = (source) =>
				ts.transpileModule(readFileSync(join(source, relative), "utf8"), {
					fileName: name,
					compilerOptions: {
						target: ts.ScriptTarget.ES2022,
						module: ts.ModuleKind.ESNext,
						sourceMap: false,
						inlineSourceMap: false,
					},
				}).outputText;
			assert.deepEqual(
				Buffer.from(emit(reference)),
				Buffer.from(emit(lock.worktree)),
				`${name} emitted JavaScript changed`,
			);
		}
		const catalog = join(
			reference,
			"packages/ai/src/providers/data/opencode-go.json",
		);
		const savedCatalog = readFileSync(catalog);
		writeFileSync(catalog, "{}\n");
		assert.match(check().output, /model snapshot mismatch/);
		writeFileSync(catalog, savedCatalog);
		const provider = join(
			reference,
			"packages/ai/src/providers/opencode-go.ts",
		);
		const saved = readFileSync(provider);
		writeFileSync(
			provider,
			Buffer.concat([saved, Buffer.from("\n// dirty\n")]),
		);
		assert.match(check().output, /dirty/);
		writeFileSync(provider, saved);
		success(command("git", ["checkout", "--detach", lock.commit], reference));
		assert.match(check().output, /HEAD mismatch/);
		success(
			command(
				"git",
				["checkout", "--detach", configured.typecheckReference.commit],
				reference,
			),
		);
		// A different parent with the same corrected tree is not the approved reference.
		const wrong = command(
			"git",
			[
				"-c",
				"user.name=Test",
				"-c",
				"user.email=test@example.invalid",
				"commit-tree",
				`${configured.typecheckReference.commit}^{tree}`,
				"-m",
				"wrong base",
			],
			reference,
		);
		success(wrong);
		success(
			command("git", ["checkout", "--detach", wrong.stdout.trim()], reference),
		);
		configured.typecheckReference.commit = wrong.stdout.trim();
		writeFileSync(join(docs, "source-lock.json"), JSON.stringify(configured));
		assert.match(check().output, /parent|base/);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});

test("locked reference verification ignores inherited diff context", () => {
	for (const context of ["0", "20"]) {
		const checked = spawnSync(process.execPath, [helper, "check"], {
			cwd: root,
			encoding: "utf8",
			env: {
				...process.env,
				GIT_CONFIG_COUNT: "1",
				GIT_CONFIG_KEY_0: "diff.context",
				GIT_CONFIG_VALUE_0: context,
			},
		});
		assert.ifError(checked.error);
		assert.equal(checked.status, 0, checked.stdout + checked.stderr);
	}
});

test("truncation exact source pin, negative proofs, and runtime imports retain their boundaries", () => {
	success(
		command(process.execPath, [
			join(root, "oracle/coding-agent-truncation-constants-typecheck.mjs"),
		]),
	);
});

test("real public-index shape and negative proofs use the selected reference", () => {
	success(
		command(process.execPath, [
			join(root, "oracle/public-index-typecheck.mjs"),
		]),
	);
});

test("entrypoint type config selects reference but embedded runtime fixture retains original", () => {
	const generated = command(process.execPath, [
		join(root, "oracle/oracle-tsconfig.mjs"),
		join(root, "oracle"),
		"protocol-oracle.ts",
	]);
	success(generated);
	const config = JSON.parse(generated.stdout);
	assert.ok(
		config.compilerOptions.paths["@earendil-works/pi-ai"].every((target) =>
			target.startsWith(lock.typecheckReference.worktree + "/"),
		),
	);
	const directory = mkdtempSync(join(tmpdir(), "pi-runtime-selection-test-"));
	try {
		const fixture = join(directory, "probe.mjs");
		writeFileSync(
			fixture,
			`console.log(${JSON.stringify(lock.typecheckReference.worktree)});\n`,
		);
		const prepared = command(process.execPath, [
			"--input-type=module",
			"-e",
			`import { prepareRuntimeFixture, selectTypecheckReference } from ${JSON.stringify(helper)}; prepareRuntimeFixture(${JSON.stringify(fixture)}, selectTypecheckReference());`,
		]);
		success(prepared);
		const runtime = command(process.execPath, [fixture]);
		success(runtime);
		assert.equal(runtime.stdout.trim(), lock.worktree);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
});
