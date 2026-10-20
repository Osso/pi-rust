// Run a compiler without filtering diagnostics or replacing its exit status.
// CLI: node oracle/typecheck-report.mjs <compiler> [arguments...]
import { spawnSync } from "node:child_process";
import { constants } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function runCompiler(command, args, cwd) {
	const result = spawnSync(command, args, { cwd, stdio: "inherit" });
	if (result.error) {
		throw new Error(
			`compiler ${command} failed to start: ${result.error.message}`,
		);
	}
	if (result.signal) {
		process.stderr.write(
			`compiler ${command} terminated by ${result.signal}\n`,
		);
		return 128 + constants.signals[result.signal];
	}
	if (result.status === null) {
		throw new Error(`compiler ${command} returned no exit status`);
	}
	return result.status;
}

if (
	process.argv[1] &&
	resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
	const [command, ...args] = process.argv.slice(2);
	if (!command) {
		console.error(
			"usage: node oracle/typecheck-report.mjs <compiler> [arguments...]",
		);
		process.exitCode = 2;
	} else {
		try {
			process.exitCode = runCompiler(command, args, process.cwd());
		} catch (error) {
			console.error(error instanceof Error ? error.message : String(error));
			process.exitCode = 1;
		}
	}
}
