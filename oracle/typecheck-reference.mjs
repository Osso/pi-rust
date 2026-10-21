// Type proof source selection only. Runtime oracles retain source-lock.worktree.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
	cpSync,
	existsSync,
	readFileSync,
	readdirSync,
	realpathSync,
	symlinkSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const catalogPath = "packages/ai/src/providers/data";
const message = "Type-only oracle reference for locked provider catalog";
function git(directory, args, env = {}) {
	const result = spawnSync(
		"git",
		["--no-optional-locks", "-C", directory, ...args],
		{ encoding: "utf8", env: { ...process.env, ...env } },
	);
	if (result.error)
		throw new Error(`git failed to start: ${result.error.message}`);
	if (result.status !== 0)
		throw new Error(`git ${args.join(" ")} failed: ${result.stderr}`);
	return result.stdout;
}
function load(repoRoot) {
	const lock = JSON.parse(
		readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"),
	);
	const reference = lock.typecheckReference;
	if (reference?.status === "blocked")
		throw new Error(`typecheck reference blocked: ${reference.todo}; see docs/porting/oracle-gate.md`);
	if (!reference)
		throw new Error(
			"missing typecheckReference metadata; restore source-lock.json",
		);
	if (reference.baseCommit !== lock.commit)
		throw new Error(
			"typecheck reference base differs from original source pin",
		);
	const patch = readFileSync(join(repoRoot, reference.patch));
	if (
		createHash("sha256").update(patch).digest("hex") !== reference.patchSha256
	)
		throw new Error("typecheck reference patch hash mismatch");
	return { lock, reference, patch };
}
function verifyOriginal(lock) {
	if (git(lock.worktree, ["rev-parse", "HEAD"]).trim() !== lock.commit)
		throw new Error("original source HEAD mismatch");
	if (
		git(lock.worktree, [
			"status",
			"--porcelain",
			"--untracked-files=normal",
		]).trim()
	)
		throw new Error("original source is dirty");
}
function verifyReferenceTree(lock, reference, patch) {
	const path = reference.worktree;
	if (!existsSync(path))
		throw new Error(
			`missing typecheck reference: ${path}; run node oracle/typecheck-reference.mjs setup`,
		);
	if (realpathSync(path) === realpathSync(lock.worktree))
		throw new Error(
			"typecheck reference must be separate from original source",
		);
	if (git(path, ["rev-parse", "HEAD"]).trim() !== reference.commit)
		throw new Error(
			"typecheck reference HEAD mismatch; run node oracle/typecheck-reference.mjs setup in a fresh reference path",
		);
	if (git(path, ["show", "-s", "--format=%P", "HEAD"]).trim() !== lock.commit)
		throw new Error("typecheck reference parent/base mismatch");
	if (git(path, ["rev-parse", "HEAD^{tree}"]).trim() !== reference.tree)
		throw new Error("typecheck reference tree mismatch");
	// Directory-only upstream ignores do not cover our dependency symlinks.
	// Their exact destinations are verified separately, not accepted as source changes.
	if (
		git(path, [
			"status",
			"--porcelain",
			"--untracked-files=normal",
			"--",
			".",
			":(exclude)node_modules",
			":(exclude)packages/*/node_modules",
		]).trim()
	)
		throw new Error("typecheck reference is dirty");
	const actual = Buffer.from(
		git(path, [
			"diff",
			"--binary",
			"--full-index",
			"--unified=6",
			lock.commit,
			"HEAD",
			"--",
		]),
	);
	if (!actual.equals(patch))
		throw new Error("typecheck reference patch mismatch");
}
function catalogHash(worktree) {
	const directory = join(worktree, catalogPath);
	const hash = createHash("sha256");
	for (const name of readdirSync(directory).sort())
		hash
			.update(name)
			.update("\0")
			.update(readFileSync(join(directory, name)))
			.update("\0");
	return hash.digest("hex");
}
function dependencyPaths(lock) {
	return [
		"node_modules",
		...readdirSync(join(lock.worktree, "packages")).map(
			(name) => `packages/${name}/node_modules`,
		),
	];
}
function installEnvironment(lock, reference) {
	for (const relative of dependencyPaths(lock)) {
		const original = join(lock.worktree, relative);
		const target = join(reference.worktree, relative);
		if (existsSync(original) && !existsSync(target))
			symlinkSync(original, target, "dir");
	}
	const target = join(reference.worktree, catalogPath);
	if (!existsSync(target))
		cpSync(join(lock.worktree, catalogPath), target, { recursive: true });
}
function verifyEnvironment(lock, reference) {
	for (const worktree of [lock.worktree, reference.worktree]) {
		if (catalogHash(worktree) !== reference.modelCatalogSha256)
			throw new Error(`locked model snapshot mismatch: ${worktree}`);
	}
	for (const relative of dependencyPaths(lock)) {
		const original = join(lock.worktree, relative);
		if (!existsSync(original)) continue;
		if (
			!existsSync(join(reference.worktree, relative)) ||
			realpathSync(join(reference.worktree, relative)) !==
				realpathSync(original)
		) {
			throw new Error(
				`reference dependency link mismatch: ${relative}; run node oracle/typecheck-reference.mjs setup`,
			);
		}
	}
}
export function selectTypecheckReference(repoRoot = root) {
	const { lock, reference, patch } = load(repoRoot);
	verifyOriginal(lock);
	verifyReferenceTree(lock, reference, patch);
	verifyEnvironment(lock, reference);
	return {
		...lock,
		worktree: reference.worktree,
		commit: reference.commit,
		runtimeWorktree: lock.worktree,
	};
}
export function setupTypecheckReference(repoRoot = root) {
	const { lock, reference, patch } = load(repoRoot);
	verifyOriginal(lock);
	if (catalogHash(lock.worktree) !== reference.modelCatalogSha256)
		throw new Error("original locked model snapshot mismatch");
	if (!existsSync(reference.worktree)) {
		git(repoRoot, [
			"clone",
			"--shared",
			"--no-checkout",
			lock.worktree,
			reference.worktree,
		]);
		git(reference.worktree, ["checkout", "--detach", lock.commit]);
		git(reference.worktree, ["apply", join(repoRoot, reference.patch)]);
		git(reference.worktree, [
			"add",
			"packages/ai/src/providers/cloudflare-ai-gateway.ts",
			"packages/ai/src/providers/opencode-go.ts",
		]);
		git(
			reference.worktree,
			[
				"-c",
				"user.name=Pi Oracle Reference",
				"-c",
				"user.email=oracle-reference@example.invalid",
				"-c",
				"commit.gpgsign=false",
				"commit",
				"-m",
				message,
			],
			{
				GIT_AUTHOR_DATE: reference.commitDate,
				GIT_COMMITTER_DATE: reference.commitDate,
			},
		);
	}
	verifyReferenceTree(lock, reference, patch);
	installEnvironment(lock, reference);
	return selectTypecheckReference(repoRoot);
}
// Embedded value probes are runtime tests, even when hosted by a typeproof.
// Rewrite only their temporary fixture, never the source tree or locked configs.
export function prepareRuntimeFixture(fixturePath, selected) {
	verifyOriginal({
		worktree: selected.runtimeWorktree,
		commit: selected.typecheckReference.baseCommit,
	});
	const source = readFileSync(fixturePath, "utf8");
	writeFileSync(
		fixturePath,
		source.replaceAll(selected.worktree, selected.runtimeWorktree),
	);
	return fixturePath;
}
if (
	process.argv[1] &&
	resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
	try {
		const [action, repoRoot = root] = process.argv.slice(2);
		if (action !== "setup" && action !== "check")
			throw new Error(
				"usage: node oracle/typecheck-reference.mjs {setup|check} [repo-root]",
			);
		const selected =
			action === "setup"
				? setupTypecheckReference(repoRoot)
				: selectTypecheckReference(repoRoot);
		console.log(selected.worktree);
	} catch (error) {
		console.error(`typecheck reference: ${error.message}`);
		process.exitCode = 1;
	}
}
