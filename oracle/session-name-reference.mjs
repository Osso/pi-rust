// Explicit v15 runtime preflight; never prepares or substitutes a reference.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

export function validateSessionNameReference(repoRoot = root) {
	const lock = JSON.parse(
		readFileSync(
			join(repoRoot, "docs/porting/source-lock.json"),
			"utf8",
		),
	);
	const reference = resolve(repoRoot, lock.worktree);
	if (!existsSync(reference))
		throw new Error(`missing v15 reference: ${reference}`);
	const head = spawnSync(
		"git",
		["--no-optional-locks", "-C", reference, "rev-parse", "HEAD"],
		{
			encoding: "utf8",
			timeout: 5000,
		},
	);
	if (head.error)
		throw new Error(`cannot verify v15 reference HEAD: ${head.error.message}`);
	if (head.status !== 0 || head.stdout.trim() !== lock.commit) {
		throw new Error(
			`v15 reference HEAD mismatch: expected ${lock.commit}, got ${head.stdout.trim() || head.stderr.trim()}`,
		);
	}
	for (const path of [
		"node_modules/.bin/tsx",
		"tsconfig.json",
		"packages/ai/src/models.generated.ts",
	]) {
		if (!existsSync(join(reference, path)))
			throw new Error(`missing v15 reference environment: ${path}`);
	}
	return reference;
}

if (
	process.argv[1] &&
	resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
	try {
		console.log(validateSessionNameReference(process.argv[2] ?? root));
	} catch (error) {
		console.error(
			`session-name reference: ${error.message}\nPrepare the checkout explicitly using docs/porting/session-name-oracle.md; no reference is created or substituted by tests.`,
		);
		process.exitCode = 1;
	}
}
