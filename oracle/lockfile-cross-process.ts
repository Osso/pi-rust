// Cross-implementation lock peer: drives the real `proper-lockfile` 4.1.2 from the pinned source
// worktree so the Rust port can prove protocol compatibility in both directions.
//
// usage: tsx lockfile-cross-process.ts <unused-module> <source-worktree-root> <mode> <file> [ms]
//   hold <file> <ms>  acquire, print LOCKED, sleep, release, print RELEASED
//   try  <file>       acquire once (retries 0): print ACQUIRED then RELEASED, or ELOCKED
import { createRequire } from "node:module";
import { setTimeout as sleep } from "node:timers/promises";

type Lockfile = {
	lock(file: string, options: { realpath: boolean; retries: number; stale?: number }): Promise<() => Promise<void>>;
};

const report = (line: string) => process.stdout.write(`${line}\n`);

async function main(): Promise<void> {
	const [, , , sourceRoot, mode, file, milliseconds] = process.argv;
	if (!sourceRoot || !mode || !file) throw new Error("lockfile peer requires <source-root> <mode> <file>");
	const require = createRequire(`${sourceRoot}/packages/coding-agent/package.json`);
	const lockfile = require("proper-lockfile") as Lockfile;

	if (mode === "hold") {
		const release = await lockfile.lock(file, { realpath: false, retries: 0, stale: 10_000 });
		report("LOCKED");
		await sleep(Number(milliseconds ?? 500));
		await release();
		report("RELEASED");
		return;
	}
	if (mode === "try") {
		try {
			const release = await lockfile.lock(file, { realpath: false, retries: 0 });
			report("ACQUIRED");
			await release();
			report("RELEASED");
		} catch (error) {
			report(`ELOCKED ${(error as { code?: string }).code}`);
		}
		return;
	}
	throw new Error(`unknown lockfile peer mode: ${mode}`);
}

void main().catch((error: unknown) => {
	process.stderr.write(`lockfile peer failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
