import { selectTypecheckReference, prepareRuntimeFixture } from "./typecheck-reference.mjs";
import { createRequire } from "node:module";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = selectTypecheckReference(repoRoot);
const sourceWorktree = sourceLock.worktree;
const publicIndexPath = join(sourceWorktree, "packages/coding-agent/src/index.ts");
const definingModulePath = join(sourceWorktree, "packages/coding-agent/src/core/session-tool-output.ts");
const highlightDeclarationPath = join(
	sourceWorktree,
	"packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const typescriptPath = join(sourceWorktree, "node_modules/typescript/lib/typescript.js");
const tsxPath = join(sourceLock.runtimeWorktree, "node_modules/.bin/tsx");
const require = createRequire(import.meta.url);
const ts = require(typescriptPath);

const expectedMax = 1_048_576;
const expectedMarker = "\n[tool output truncated at 1 MiB]";

const runtimeProbeSource = `
import {
  MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES,
  TOOL_RESULT_TRUNCATION_MARKER,
} from ${JSON.stringify(publicIndexPath)};

if (MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES !== ${expectedMax}) {
  throw new Error(
    "MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES must equal ${expectedMax}, got " +
      MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES,
  );
}
if (TOOL_RESULT_TRUNCATION_MARKER !== ${JSON.stringify(expectedMarker)}) {
  throw new Error(
    "TOOL_RESULT_TRUNCATION_MARKER has an unexpected value: " + TOOL_RESULT_TRUNCATION_MARKER,
  );
}

console.log(
  JSON.stringify({
    maxPersistedToolResultContentBytes: MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES,
    toolResultTruncationMarker: TOOL_RESULT_TRUNCATION_MARKER,
  }),
);
`;

function validateSourceLock() {
	if (typeof sourceWorktree !== "string" || sourceWorktree.length === 0) {
		throw new Error("source lock worktree must be a non-empty string");
	}
	if (typeof sourceLock.commit !== "string" || !/^[0-9a-f]{40}$/.test(sourceLock.commit)) {
		throw new Error("source lock commit must be a 40-character hexadecimal string");
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

function formatDiagnostics(diagnostics) {
	return ts.formatDiagnosticsWithColorAndContext(diagnostics, {
		getCanonicalFileName: (fileName) => fileName,
		getCurrentDirectory: () => sourceWorktree,
		getNewLine: () => "\n",
	});
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

function assertIndexAlias(checker, indexFile, definingFile, exports, name) {
	const exportedSymbol = exports.find((symbol) => symbol.name === name);
	if (exportedSymbol === undefined) throw new Error(`real index does not export ${name}`);
	if ((exportedSymbol.flags & ts.SymbolFlags.Alias) === 0) {
		throw new Error(`${name} is not represented as a real index alias`);
	}
	const publicDeclarations = exportedSymbol.declarations ?? [];
	if (
		publicDeclarations.length === 0 ||
		publicDeclarations.some((declaration) => declaration.getSourceFile() !== indexFile)
	) {
		throw new Error(`${name} has a public declaration outside the real index`);
	}
	const target = checker.getAliasedSymbol(exportedSymbol);
	const targetDeclarations = target.declarations ?? [];
	if (
		targetDeclarations.length === 0 ||
		targetDeclarations.some((declaration) => declaration.getSourceFile() !== definingFile)
	) {
		throw new Error(`${name} does not resolve to the real defining module`);
	}
	return exportedSymbol;
}

function assertNumberLiteral(checker, symbol, indexFile) {
	const type = checker.getTypeOfSymbolAtLocation(symbol, indexFile);
	if ((type.flags & ts.TypeFlags.NumberLiteral) === 0 || type.value !== expectedMax) {
		throw new Error(`${symbol.name} is not the exact number literal ${expectedMax}`);
	}
}

function assertStringLiteral(checker, symbol, indexFile) {
	const type = checker.getTypeOfSymbolAtLocation(symbol, indexFile);
	if ((type.flags & ts.TypeFlags.StringLiteral) === 0 || type.value !== expectedMarker) {
		throw new Error(`${symbol.name} is not the exact expected string literal`);
	}
}

function createSymbolScopedProgram() {
	const options = readPinnedTsConfig();
	return ts.createProgram({
		rootNames: [publicIndexPath, highlightDeclarationPath],
		options,
	});
}

function getRealIndexExports(checker, indexFile) {
	const indexModule = checker.getSymbolAtLocation(indexFile);
	if (indexModule === undefined) throw new Error("could not obtain the real coding-agent index module symbol");
	return checker.getExportsOfModule(indexModule);
}

function assertConstantExports(checker, indexFile, definingFile) {
	const exports = getRealIndexExports(checker, indexFile);
	const maxSymbol = assertIndexAlias(
		checker,
		indexFile,
		definingFile,
		exports,
		"MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES",
	);
	const markerSymbol = assertIndexAlias(
		checker,
		indexFile,
		definingFile,
		exports,
		"TOOL_RESULT_TRUNCATION_MARKER",
	);
	assertNumberLiteral(checker, maxSymbol, indexFile);
	assertStringLiteral(checker, markerSymbol, indexFile);
}

function runCompilerProof() {
	const program = createSymbolScopedProgram();
	const indexFile = sourceFileAt(program, publicIndexPath, "coding-agent index source file");
	const definingFile = sourceFileAt(program, definingModulePath, "tool-output constants source file");
	assertNoSourceDiagnostics(program, indexFile, "coding-agent index");
	assertNoSourceDiagnostics(program, definingFile, "tool-output constants module");
	assertConstantExports(program.getTypeChecker(), indexFile, definingFile);

	return {
		compilerVersion: ts.version,
		indexDiagnostics: 0,
		definingModuleDiagnostics: 0,
		maxType: expectedMax,
		markerType: expectedMarker,
	};
}

function runProcessOrThrow(command, args) {
	const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8" });
	if (result.stdout !== "") process.stdout.write(result.stdout);
	if (result.stderr !== "") process.stderr.write(result.stderr);
	if (result.error !== undefined) throw new Error(`${command} failed to start: ${result.error.message}`);
	if (result.status !== 0) throw new Error(`${command} exited with status ${result.status ?? 1}`);
}

function runRuntimeProbe() {
	const directory = mkdtempSync(join(tmpdir(), "pi-tool-output-persistence-constants-runtime-"));
	const fixturePath = join(directory, "tool-output-persistence-constants-runtime.mjs");
	writeFileSync(fixturePath, runtimeProbeSource, "utf8");
	try {
		runProcessOrThrow(tsxPath, ["--tsconfig", join(sourceLock.runtimeWorktree, "tsconfig.json"), prepareRuntimeFixture(fixturePath, sourceLock)]);
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
}

function main() {
	validateSourceLock();
	const proof = runCompilerProof();
	console.log(`compiler=${proof.compilerVersion}`);
	console.log(
		`symbol-scoped diagnostics: index=${proof.indexDiagnostics}, defining-module=${proof.definingModuleDiagnostics}`,
	);
	console.log(`types: MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES=${proof.maxType}`);
	console.log(`types: TOOL_RESULT_TRUNCATION_MARKER=${JSON.stringify(proof.markerType)}`);
	runRuntimeProbe();
}

try {
	main();
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`tool-output persistence constants typeproof failed: ${message}\n`);
	process.exitCode = 1;
}
