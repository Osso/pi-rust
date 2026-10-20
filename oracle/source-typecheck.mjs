// Runs a pinned-source typecheck fixture and judges only our own diagnostics.
//
// Each `*-typecheck.mjs` fixture imports a pinned source module. The source
// resolves its `@earendil-works/*` imports through `dist/`, which is absent in
// the read-only checkout, so the fixture is compiled through the shared
import { buildSourceTsconfig } from "./source-tsconfig.mjs";
// source-paths tsconfig. The pinned source itself has pre-existing type errors,
// so only diagnostics inside this repository fail the gate; the rest are
// reported as ignored (see `typecheck-report.mjs`).
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";


export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** The pinned source worktree recorded in `docs/porting/source-lock.json`. */
export function readSourceWorktree() {
	return JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8")).worktree;
}

/**
 * Typechecks `fixture` (a module body) in a temp dir against the pinned source,
 * returning the gate's exit status.
 */
export function typecheckFixture(sourceWorktree, name, fixture) {
	const directory = mkdtempSync(join(tmpdir(), `pi-${name}-typecheck-`));
	const fixturePath = join(directory, `${name}.ts`);
	const configPath = join(directory, "tsconfig.json");
	writeFileSync(fixturePath, fixture, "utf8");
	writeFileSync(
		configPath,
		`${JSON.stringify(buildSourceTsconfig(sourceWorktree, [fixturePath]), null, 2)}\n`,
		"utf8",
	);
	try {
		const result = spawnSync(join(sourceWorktree, "node_modules/.bin/tsgo"), ["-p", configPath], {
			cwd: repoRoot,
			encoding: "utf8",
		});
		if (result.error !== undefined) throw new Error(`pinned tsgo failed to start: ${result.error.message}`);
		const report = spawnSync(process.execPath, [join(repoRoot, "oracle/typecheck-report.mjs"), repoRoot], {
			input: `${result.stdout ?? ""}${result.stderr ?? ""}`,
			encoding: "utf8",
		});
		if (report.stdout) process.stdout.write(report.stdout);
		if (report.stderr) process.stderr.write(report.stderr);
		return report.status ?? 1;
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
}
