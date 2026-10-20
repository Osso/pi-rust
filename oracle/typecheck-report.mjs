// Filters pinned-source typecheck output down to diagnostics in our own files.
//
// The pinned TypeScript source does not typecheck cleanly on its own (48
// pre-existing errors, e.g. `packages/ai/src/providers/cloudflare-ai-gateway.ts`
// and the model-catalog tests). The oracle gate must still fail on errors in the
// oracle fixtures and typecheck scripts, so this keeps diagnostics whose path is
// inside this repository and reports how many source diagnostics were ignored.
//
// Usage: tsgo ... 2>&1 | node oracle/typecheck-report.mjs <repo-root>
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const repoRoot = resolve(process.argv[2] ?? ".");
const input = readFileSync(0, "utf8");

const diagnostic = /^(.+?)\(\d+,\d+\): error TS\d+:/;
const kept = [];
const ignored = [];
for (const line of input.split("\n")) {
	const match = diagnostic.exec(line);
	if (!match) continue;
	(resolve(match[1]).startsWith(repoRoot) ? kept : ignored).push(line);
}

if (ignored.length > 0) {
	process.stdout.write(
		`oracle typecheck: ${ignored.length} pre-existing pinned-source diagnostic(s) ignored\n`,
	);
}
for (const line of kept) process.stderr.write(`${line}\n`);
process.exitCode = kept.length === 0 ? 0 : 1;
