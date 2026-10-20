// Emits the tsconfig used by `run-tests.sh oracle` for the oracle files.
//
// Usage: node oracle/oracle-tsconfig.mjs <oracle-dir> <file>...
import { join, resolve } from "node:path";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";
import { selectTypecheckReference } from "./typecheck-reference.mjs";

const [oracleDir, ...files] = process.argv.slice(2);
if (!oracleDir || files.length === 0) {
	console.error(
		"usage: node oracle-tsconfig.mjs <oracle-dir> <oracle-file>...",
	);
	process.exit(2);
}

const { worktree: sourceWorktree } = selectTypecheckReference();
const resolved = files.map((file) => join(resolve(oracleDir), file));
process.stdout.write(
	`${JSON.stringify(buildSourceTsconfig(sourceWorktree, resolved), null, 2)}\n`,
);
