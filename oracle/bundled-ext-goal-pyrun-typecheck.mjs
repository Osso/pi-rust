import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { selectTypecheckReference } from "./typecheck-reference.mjs";
import { buildSourceTsconfig } from "./source-tsconfig.mjs";
import { runCompiler } from "./typecheck-report.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = selectTypecheckReference(root).worktree;
const fixtures = [
	"goal-management-oracle.ts",
	"goal-scheduling-oracle.ts",
	"goal-completion-oracle.ts",
	"goal-agent-end-oracle.ts",
	"goal-countdown-oracle.ts",
	"goal-rendering-oracle.ts",
	"goal-supervisor-review-oracle.ts",
	"goal-idle-selection-oracle.ts",
	"goal-review-evidence-oracle.ts",
	"pyrun-artifacts-oracle.ts",
	"pyrun-progress-oracle.ts",
	"pyrun-registration-oracle.ts",
	"pyrun-runner-queue-oracle.ts",
	"pyrun-bridge-oracle.ts",
].map((name) => join(root, "oracle", name));
const temporary = mkdtempSync(join(tmpdir(), "bundled-ext-typecheck-"));
try {
	const config = buildSourceTsconfig(source, fixtures);
	// Approved scoped noEmit target for frozen TUI /v regex; see oracle-gate.md.
	config.compilerOptions.target = "ES2024";
	config.compilerOptions.lib = ["ES2024", "DOM", "DOM.Iterable"];
	const path = join(temporary, "tsconfig.json");
	writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`);
	process.exitCode = runCompiler(
		join(source, "node_modules/.bin/tsgo"),
		["-p", path],
		root,
	);
} finally {
	rmSync(temporary, { recursive: true });
}
