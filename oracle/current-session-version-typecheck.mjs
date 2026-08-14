import { createRequire } from "node:module";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const sourceWorktree = sourceLock.worktree;
const publicIndexPath = join(sourceWorktree, "packages/coding-agent/src/index.ts");
const definingModulePath = join(sourceWorktree, "packages/coding-agent/src/core/session-manager.ts");
const highlightDeclarationPath = join(
	sourceWorktree,
	"packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const typescriptPath = join(sourceWorktree, "node_modules/typescript/lib/typescript.js");
const tsxPath = join(sourceWorktree, "node_modules/.bin/tsx");
const require = createRequire(import.meta.url);
const ts = require(typescriptPath);
const expectedValue = 3;

const runtimeProbeSource = `
import { CURRENT_SESSION_VERSION } from ${JSON.stringify(publicIndexPath)};

if (CURRENT_SESSION_VERSION !== ${expectedValue}) {
  throw new Error(
    "CURRENT_SESSION_VERSION must equal ${expectedValue}, got " + CURRENT_SESSION_VERSION,
  );
}

console.log(JSON.stringify({ currentSessionVersion: CURRENT_SESSION_VERSION }));
`;

function formatDiagnostics(diagnostics) {
	return ts.formatDiagnosticsWithColorAndContext(diagnostics, {
		getCanonicalFileName: (fileName) => fileName,
		getCurrentDirectory: () => sourceWorktree,
		getNewLine: () => "\n",
	});
}

function validateSourceLock() {
	if (typeof sourceWorktree !== "string" || sourceWorktree.length === 0) {
		throw new Error("source lock worktree must be a non-empty string");
	}
	if (typeof sourceLock.commit !== "string" || !/^[0-9a-f]{40}$/.test(sourceLock.commit)) {
		throw new Error("source lock commit must be a 40-character hexadecimal string");
	}
}

function verifyPinnedSourceHead() {
	const result = spawnSync("git", ["-C", sourceWorktree, "rev-parse", "HEAD"], { encoding: "utf8" });
	if (result.error !== undefined) throw new Error(`source HEAD check failed to start git: ${result.error.message}`);
	if (result.status !== 0) throw new Error(`source HEAD check failed: ${result.stderr.trim()}`);
	const actualCommit = result.stdout.trim();
	if (actualCommit !== sourceLock.commit) {
		throw new Error(`pinned TypeScript source HEAD mismatch: expected ${sourceLock.commit}, got ${actualCommit}`);
	}
}

function readPinnedTsConfig() {
	const configFile = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
	if (configFile.error !== undefined) {
		throw new Error(`pinned tsconfig read failed: ${formatDiagnostics([configFile.error])}`);
	}
	const parsed = ts.parseJsonConfigFileContent(
		configFile.config,
		ts.sys,
		sourceWorktree,
		undefined,
		tsconfigPath,
	);
	if (parsed.errors.length > 0) {
		throw new Error(`pinned tsconfig parse failed: ${formatDiagnostics(parsed.errors)}`);
	}
	if (parsed.options.strict !== true || parsed.options.skipLibCheck !== true) {
		throw new Error("pinned tsconfig must retain strict and skipLibCheck=true");
	}
	return parsed.options;
}

function sourceFileAt(program, path, label) {
	const sourceFile = program.getSourceFile(path);
	if (sourceFile === undefined) throw new Error(`could not load real ${label}: ${path}`);
	return sourceFile;
}

function assertNoSourceDiagnostics(program, sourceFile, label) {
	const diagnostics = [
		...program.getSyntacticDiagnostics(sourceFile),
		...program.getSemanticDiagnostics(sourceFile),
	];
	if (diagnostics.length > 0) {
		throw new Error(`${label} symbol-scoped diagnostics failed:\n${formatDiagnostics(diagnostics)}`);
	}
}

function moduleExports(checker, sourceFile, label) {
	const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
	if (moduleSymbol === undefined) throw new Error(`could not obtain ${label} module symbol`);
	return checker.getExportsOfModule(moduleSymbol);
}

function assertIndexAlias(checker, indexFile, definingFile) {
	const indexExports = moduleExports(checker, indexFile, "real coding-agent index");
	const publicSymbol = indexExports.find((symbol) => symbol.name === "CURRENT_SESSION_VERSION");
	if (publicSymbol === undefined) throw new Error("real coding-agent index does not export CURRENT_SESSION_VERSION");
	if ((publicSymbol.flags & ts.SymbolFlags.Alias) === 0) {
		throw new Error("CURRENT_SESSION_VERSION is not represented as a real index alias");
	}
	const publicDeclarations = publicSymbol.declarations ?? [];
	if (publicDeclarations.length === 0 || publicDeclarations.some((declaration) => declaration.getSourceFile() !== indexFile)) {
		throw new Error("CURRENT_SESSION_VERSION has a public declaration outside the real index");
	}
	const target = checker.getAliasedSymbol(publicSymbol);
	const targetDeclarations = target.declarations ?? [];
	if (targetDeclarations.length === 0 || targetDeclarations.some((declaration) => declaration.getSourceFile() !== definingFile)) {
		throw new Error("CURRENT_SESSION_VERSION does not resolve to session-manager.ts");
	}

	const definingExports = moduleExports(checker, definingFile, "real session-manager");
	if (!definingExports.some((symbol) => symbol.name === "CURRENT_SESSION_VERSION")) {
		throw new Error("session-manager.ts does not export CURRENT_SESSION_VERSION");
	}

	const type = checker.getTypeOfSymbolAtLocation(publicSymbol, indexFile);
	if ((type.flags & ts.TypeFlags.NumberLiteral) === 0 || type.value !== expectedValue) {
		throw new Error(`CURRENT_SESSION_VERSION is not the exact number literal ${expectedValue}`);
	}
}

function runCompilerProof() {
	const options = readPinnedTsConfig();
	const program = ts.createProgram({
		rootNames: [publicIndexPath, highlightDeclarationPath],
		options,
	});
	const indexFile = sourceFileAt(program, publicIndexPath, "coding-agent index source file");
	const definingFile = sourceFileAt(program, definingModulePath, "session-manager source file");
	assertNoSourceDiagnostics(program, indexFile, "coding-agent index");
	assertNoSourceDiagnostics(program, definingFile, "session-manager module");
	assertIndexAlias(program.getTypeChecker(), indexFile, definingFile);
	return { compilerVersion: ts.version };
}

function runRuntimeProbe() {
	const directory = mkdtempSync(join(tmpdir(), "pi-current-session-version-runtime-"));
	const fixturePath = join(directory, "current-session-version-runtime.mjs");
	writeFileSync(fixturePath, runtimeProbeSource, "utf8");
	try {
		const result = spawnSync(tsxPath, ["--tsconfig", tsconfigPath, fixturePath], {
			cwd: repoRoot,
			encoding: "utf8",
		});
		if (result.stdout !== "") process.stdout.write(result.stdout);
		if (result.stderr !== "") process.stderr.write(result.stderr);
		if (result.error !== undefined) throw new Error(`tsx failed to start: ${result.error.message}`);
		if (result.status !== 0) throw new Error(`tsx exited with status ${result.status ?? 1}`);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
}

try {
	validateSourceLock();
	verifyPinnedSourceHead();
	const proof = runCompilerProof();
	console.log(`compiler=${proof.compilerVersion}`);
	console.log("symbol-scoped diagnostics: index=0, defining-module=0");
	console.log("type: CURRENT_SESSION_VERSION=3");
	runRuntimeProbe();
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`current session version typeproof failed: ${message}\n`);
	process.exitCode = 1;
}
