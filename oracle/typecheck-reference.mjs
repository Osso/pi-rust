// Type and runtime proofs share the unmodified frozen source. No patched fallback.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
function git(directory, args) {
	const result = spawnSync(
		"git",
		["--no-optional-locks", "-C", directory, ...args],
		{ encoding: "utf8" },
	);
	if (result.error)
		throw new Error(`git failed to start: ${result.error.message}`);
	if (result.status !== 0)
		throw new Error(`git ${args.join(" ")} failed: ${result.stderr}`);
	return result.stdout;
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
	for (const path of [
		"node_modules/.bin/tsgo",
		"tsconfig.json",
		lock.modelCatalog.path,
	]) {
		if (!existsSync(join(lock.worktree, path)))
			throw new Error(`missing typecheck environment: ${path}`);
	}
	git(lock.worktree, ["ls-files", "--error-unmatch", lock.modelCatalog.path]);
	const original = Buffer.from(
		git(lock.worktree, ["show", `${lock.commit}:${lock.modelCatalog.path}`]),
	);
	if (
		!readFileSync(join(lock.worktree, lock.modelCatalog.path)).equals(original)
	)
		throw new Error("tracked model catalog differs from frozen source");
}
export function selectTypecheckReference(repoRoot = root) {
	const lock = JSON.parse(
		readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"),
	);
	const reference = lock.typecheckReference;
	if (reference?.status === "blocked")
		throw new Error(
			`typecheck reference blocked: ${reference.todo}; see docs/porting/oracle-gate.md`,
		);
	if (reference?.status !== "unmodified")
		throw new Error(
			"typecheck reference must explicitly select unmodified frozen source",
		);
	if (reference.baseCommit !== lock.commit)
		throw new Error("typecheck reference base differs from source pin");
	verifyOriginal(lock);
	return { ...lock, runtimeWorktree: lock.worktree };
}
// setup is validation only: no checkout, dependency, catalog or source writes.
export function setupTypecheckReference(repoRoot = root) {
	return selectTypecheckReference(repoRoot);
}
export function prepareRuntimeFixture(fixturePath, selected) {
	if (
		selected.worktree !== selected.runtimeWorktree ||
		selected.typecheckReference.baseCommit !== selected.commit
	)
		throw new Error("runtime fixture must use the unmodified typecheck source");
	verifyOriginal(selected);
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
		console.log(
			(action === "setup"
				? setupTypecheckReference(repoRoot)
				: selectTypecheckReference(repoRoot)
			).worktree,
		);
	} catch (error) {
		console.error(`typecheck reference: ${error.message}`);
		process.exitCode = 1;
	}
}
