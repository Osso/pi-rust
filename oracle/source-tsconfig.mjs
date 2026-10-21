// Shared tsconfig builder for the oracle typecheck gates.
//
// The oracles and typecheck fixtures import `@earendil-works/*` from the pinned
// source worktree. Those packages resolve through `dist/` (absent in the
// read-only checkout), so the configs inherit the source's own compiler options
// and path mapping and only swap `files`. Path targets become absolute because
// the generated configs live outside the source worktree and `baseUrl` is gone.
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

function readCompilerOptions(sourceWorktree) {
	const read = (name) => {
		const path = join(sourceWorktree, name);
		if (!existsSync(path)) throw new Error(`missing ${path}`);
		return JSON.parse(readFileSync(path, "utf8")).compilerOptions ?? {};
	};
	return { ...read("tsconfig.base.json"), ...read("tsconfig.json") };
}

export function buildSourceTsconfig(sourceWorktree, files) {
	const { paths, ...inherited } = readCompilerOptions(sourceWorktree);
	const compilerOptions = {
		...inherited,
		noEmit: true,
		// Fixtures import absolute .ts paths for checking, not JS emission.
		rewriteRelativeImportExtensions: false,
		typeRoots: [join(sourceWorktree, "node_modules", "@types")],
	};
	compilerOptions.paths = {
		...Object.fromEntries(
			Object.entries(paths ?? {}).map(([pattern, targets]) => [
				pattern,
				targets.map((target) => resolve(sourceWorktree, target)),
			]),
		),
		"@pi-oracle-source/*": [join(sourceWorktree, "packages/*")],
	};
	return {
		compilerOptions,
		files: files.map((file) => resolve(file)),
		// Retain source-provided ambient declarations in temporary fixture configs.
		include: [join(sourceWorktree, "packages/*/src/**/*.d.ts")],
	};
}
