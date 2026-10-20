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
const definingModulePath = join(sourceWorktree, "packages/coding-agent/src/core/tools/truncate.ts");
const toolsIndexPath = join(sourceWorktree, "packages/coding-agent/src/core/tools/index.ts");
const packageIndexPath = join(sourceWorktree, "packages/coding-agent/src/index.ts");
const definingImportPath = definingModulePath.slice(0, -3);
const toolsImportPath = toolsIndexPath.slice(0, -3);
const packageImportPath = packageIndexPath.slice(0, -3);
const highlightDeclarationPath = join(
	sourceWorktree,
	"packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const typescriptPath = join(sourceWorktree, "node_modules/typescript/lib/typescript.js");
const tsxPath = join(sourceLock.runtimeWorktree, "node_modules/.bin/tsx");
const require = createRequire(import.meta.url);
const ts = require(typescriptPath);

const validFixtureSource = `
import {
	DEFAULT_MAX_LINES as directMaxLines,
	DEFAULT_MAX_BYTES as directMaxBytes,
	GREP_MAX_LINE_LENGTH as directGrepMaxLineLength,
} from ${JSON.stringify(definingImportPath)};
import {
	DEFAULT_MAX_LINES as toolsMaxLines,
	DEFAULT_MAX_BYTES as toolsMaxBytes,
} from ${JSON.stringify(toolsImportPath)};
import {
	DEFAULT_MAX_LINES as packageMaxLines,
	DEFAULT_MAX_BYTES as packageMaxBytes,
} from ${JSON.stringify(packageImportPath)};

const directLinesAssignment: typeof directMaxLines = 2000;
const directBytesAssignment: typeof directMaxBytes = 10240;
const directGrepAssignment: typeof directGrepMaxLineLength = 500;
const toolsLinesAssignment: typeof toolsMaxLines = directMaxLines;
const toolsBytesAssignment: typeof toolsMaxBytes = directMaxBytes;
const packageLinesAssignment: typeof packageMaxLines = toolsMaxLines;
const packageBytesAssignment: typeof packageMaxBytes = toolsMaxBytes;
const broadNumberAssignment: number = directMaxBytes;
void directLinesAssignment;
void directBytesAssignment;
void directGrepAssignment;
void toolsLinesAssignment;
void toolsBytesAssignment;
void packageLinesAssignment;
void packageBytesAssignment;
void broadNumberAssignment;
`;

const invalidFixtureSource = `
import {
	DEFAULT_MAX_LINES as directMaxLines,
	DEFAULT_MAX_BYTES as directMaxBytes,
	GREP_MAX_LINE_LENGTH as directGrepMaxLineLength,
} from ${JSON.stringify(definingImportPath)};
import { GREP_MAX_LINE_LENGTH as toolsGrepMaxLineLength } from ${JSON.stringify(toolsImportPath)};
import { GREP_MAX_LINE_LENGTH as packageGrepMaxLineLength } from ${JSON.stringify(packageImportPath)};

const wrongLines: typeof directMaxLines = 2001;
const wrongBytes: typeof directMaxBytes = "wrong";
const wrongGrep: typeof directGrepMaxLineLength = 501;
void wrongLines;
void wrongBytes;
void wrongGrep;
void toolsGrepMaxLineLength;
void packageGrepMaxLineLength;
`;

const expectedInvalidDiagnostics = [
	{ label: "tools GREP forbidden export", code: 2305, span: "GREP_MAX_LINE_LENGTH", line: 7 },
	{ label: "package GREP forbidden export", code: 2305, span: "GREP_MAX_LINE_LENGTH", line: 8 },
	{
		label: "wrong DEFAULT_MAX_LINES literal",
		code: 2322,
		span: "wrongLines",
		line: 10,
		message: "Type '2001' is not assignable to type '2000'.",
	},
	{
		label: "wrong DEFAULT_MAX_BYTES string",
		code: 2322,
		span: "wrongBytes",
		line: 11,
		message: "Type 'string' is not assignable to type 'number'.",
	},
	{
		label: "wrong GREP_MAX_LINE_LENGTH literal",
		code: 2322,
		span: "wrongGrep",
		line: 12,
		message: "Type '501' is not assignable to type '500'.",
	},
];

const runtimeFixtureSource = `
import * as direct from ${JSON.stringify(definingModulePath)};
import * as tools from ${JSON.stringify(toolsIndexPath)};

process.stdout.write(JSON.stringify({
	direct: {
		defaultMaxLines: direct.DEFAULT_MAX_LINES,
		defaultMaxBytes: direct.DEFAULT_MAX_BYTES,
		grepMaxLineLength: direct.GREP_MAX_LINE_LENGTH,
		hasGrep: Object.hasOwn(direct, "GREP_MAX_LINE_LENGTH"),
	},
	tools: {
		defaultMaxLines: tools.DEFAULT_MAX_LINES,
		defaultMaxBytes: tools.DEFAULT_MAX_BYTES,
		hasGrep: Object.hasOwn(tools, "GREP_MAX_LINE_LENGTH"),
	},
}));
`;

const expectedRuntimeExports = {
	direct: { defaultMaxLines: 2000, defaultMaxBytes: 10240, grepMaxLineLength: 500, hasGrep: true },
	tools: { defaultMaxLines: 2000, defaultMaxBytes: 10240, hasGrep: false },
};

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
	if (sourceLock.typecheckReference.baseCommit !== "532f7d12cd19235081bd4d6e425bde185652d86f") {
		throw new Error(`source lock base commit mismatch: ${sourceLock.typecheckReference.baseCommit}`);
	}
}

function readPinnedTsConfig() {
	const configFile = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
	if (configFile.error !== undefined) {
		throw new Error(`pinned tsconfig read failed:\n${formatDiagnostics([configFile.error])}`);
	}
	const parsed = ts.parseJsonConfigFileContent(
		configFile.config,
		ts.sys,
		sourceWorktree,
		undefined,
		tsconfigPath,
	);
	if (parsed.errors.length > 0) {
		throw new Error(`pinned tsconfig parse failed:\n${formatDiagnostics(parsed.errors)}`);
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

function moduleExports(checker, sourceFile, label) {
	const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
	if (moduleSymbol === undefined) throw new Error(`could not obtain ${label} module symbol`);
	return checker.getExportsOfModule(moduleSymbol);
}

function assertNoSourceDiagnostics(program, sourceFile, label) {
	const diagnostics = [
		...program.getSyntacticDiagnostics(sourceFile),
		...program.getSemanticDiagnostics(sourceFile),
	];
	if (diagnostics.length > 0) {
		throw new Error(`${label} diagnostics failed:\n${formatDiagnostics(diagnostics)}`);
	}
}

function resolveDirectConstant(checker, definingFile, name) {
	const symbol = moduleExports(checker, definingFile, "truncate module").find((candidate) => candidate.name === name);
	if (symbol === undefined) throw new Error(`truncate.ts does not export ${name}`);
	if ((symbol.flags & ts.SymbolFlags.Alias) !== 0) {
		throw new Error(`truncate.ts ${name} must be the defining symbol, not an alias`);
	}
	const declarations = symbol.declarations ?? [];
	if (declarations.length !== 1 || declarations[0].getSourceFile() !== definingFile) {
		throw new Error(`${name} must have exactly one declaration in truncate.ts`);
	}
	const [declaration] = declarations;
	if (declaration.kind !== ts.SyntaxKind.VariableDeclaration) {
		throw new Error(`${name} must resolve to a variable declaration`);
	}
	const declarationList = declaration.parent;
	if (
		declarationList.kind !== ts.SyntaxKind.VariableDeclarationList ||
		(declarationList.flags & ts.NodeFlags.Const) === 0
	) {
		throw new Error(`${name} must be declared with const`);
	}
	return symbol;
}

function assertIndexAlias(checker, indexFile, definingFile, definingSymbol, name, label) {
	const exportedSymbol = moduleExports(checker, indexFile, label).find((candidate) => candidate.name === name);
	if (exportedSymbol === undefined) throw new Error(`${label} does not export ${name}`);
	if ((exportedSymbol.flags & ts.SymbolFlags.Alias) === 0) {
		throw new Error(`${label} ${name} is not a real index alias`);
	}
	const declarations = exportedSymbol.declarations ?? [];
	if (declarations.length === 0 || declarations.some((declaration) => declaration.getSourceFile() !== indexFile)) {
		throw new Error(`${label} ${name} has a declaration outside the real index`);
	}
	const target = checker.getAliasedSymbol(exportedSymbol);
	const targetDeclarations = target.declarations ?? [];
	if (
		target !== definingSymbol ||
		targetDeclarations.length !== 1 ||
		targetDeclarations[0].getSourceFile() !== definingFile
	) {
		throw new Error(`${label} ${name} does not alias the real truncate.ts declaration`);
	}
}

function assertAbsent(checker, indexFile, name, label) {
	if (moduleExports(checker, indexFile, label).some((candidate) => candidate.name === name)) {
		throw new Error(`${label} must not export ${name}`);
	}
}

function assertExactType(checker, symbol, sourceFile, expectedFlags, expectedText, expectedValue, label) {
	const type = checker.getTypeOfSymbolAtLocation(symbol, sourceFile);
	if (type.flags !== expectedFlags || checker.typeToString(type) !== expectedText) {
		throw new Error(`${label} must have exact type ${expectedText}, got ${checker.typeToString(type)}`);
	}
	if (expectedValue !== undefined && type.value !== expectedValue) {
		throw new Error(`${label} must have literal value ${expectedValue}`);
	}
}

function createProgramForFixture(source, options, prefix) {
	const directory = mkdtempSync(join(tmpdir(), prefix));
	const fixturePath = join(directory, "truncation-constants.ts");
	writeFileSync(fixturePath, source, "utf8");
	const program = ts.createProgram({
		rootNames: [fixturePath, definingModulePath, toolsIndexPath, packageIndexPath, highlightDeclarationPath],
		options: { ...options, noEmit: true },
	});
	return { directory, fixturePath, program };
}

function diagnosticSpan(sourceFile, diagnostic) {
	if (diagnostic.start === undefined || diagnostic.length === undefined) return "";
	return sourceFile.text.slice(diagnostic.start, diagnostic.start + diagnostic.length);
}

function diagnosticLine(sourceFile, diagnostic) {
	if (diagnostic.start === undefined) return 0;
	return sourceFile.getLineAndCharacterOfPosition(diagnostic.start).line + 1;
}

function diagnosticMessage(diagnostic) {
	return ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n");
}

function proveValidAssignments(options) {
	const fixture = createProgramForFixture(validFixtureSource, options, "pi-coding-agent-truncation-constants-valid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "valid truncation-constants fixture");
		const diagnostics = [
			...fixture.program.getSyntacticDiagnostics(sourceFile),
			...fixture.program.getSemanticDiagnostics(sourceFile),
		];
		if (diagnostics.length > 0) {
			throw new Error(`valid assignment diagnostics:\n${formatDiagnostics(diagnostics)}`);
		}
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

function collectFixtureDiagnostics(program, sourceFile) {
	return [
		...program.getSyntacticDiagnostics(sourceFile),
		...program.getSemanticDiagnostics(sourceFile),
	];
}

function assertExpectedInvalidDiagnostics(sourceFile, diagnostics) {
	if (diagnostics.length !== expectedInvalidDiagnostics.length) {
		throw new Error(
			`expected ${expectedInvalidDiagnostics.length} exact invalid diagnostics, got ${diagnostics.length}:\n${formatDiagnostics(diagnostics)}`,
		);
	}
	const actual = diagnostics.map((diagnostic) => ({
		code: diagnostic.code,
		category: diagnostic.category,
		span: diagnosticSpan(sourceFile, diagnostic),
		line: diagnosticLine(sourceFile, diagnostic),
		message: diagnosticMessage(diagnostic),
	}));
	for (const expected of expectedInvalidDiagnostics) {
		const matches = actual.filter(
			(candidate) =>
				candidate.code === expected.code &&
				candidate.category === ts.DiagnosticCategory.Error &&
				candidate.span === expected.span &&
				candidate.line === expected.line &&
				(expected.message === undefined || candidate.message === expected.message),
		);
		if (matches.length !== 1) {
			throw new Error(`missing exact ${expected.label} diagnostic: ${JSON.stringify(actual)}`);
		}
	}
	return diagnostics.length;
}

function proveInvalidDiagnostics(options) {
	const fixture = createProgramForFixture(invalidFixtureSource, options, "pi-coding-agent-truncation-constants-invalid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "invalid truncation-constants fixture");
		return assertExpectedInvalidDiagnostics(sourceFile, collectFixtureDiagnostics(fixture.program, sourceFile));
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

function parseRuntimeResult(result) {
	if (result.error !== undefined) throw new Error(`runtime proof failed to start Node: ${result.error.message}`);
	if (result.status !== 0) throw new Error(`runtime proof failed: ${result.stderr.trim()}`);
	return JSON.parse(result.stdout.trim());
}

function proveRuntimeImports() {
	const directory = mkdtempSync(join(tmpdir(), "pi-coding-agent-truncation-constants-runtime-"));
	const fixturePath = join(directory, "runtime.ts");
	writeFileSync(fixturePath, runtimeFixtureSource, "utf8");
	try {
		const result = spawnSync(tsxPath, ["--tsconfig", join(sourceLock.runtimeWorktree, "tsconfig.json"), prepareRuntimeFixture(fixturePath, sourceLock)], {
			cwd: repoRoot,
			encoding: "utf8",
		});
		const actual = parseRuntimeResult(result);
		if (JSON.stringify(actual) !== JSON.stringify(expectedRuntimeExports)) {
			throw new Error(`runtime export mismatch: ${JSON.stringify(actual)}`);
		}
		return actual;
	} finally {
		rmSync(directory, { recursive: true, force: true });
	}
}

try {
	validateSourceLock();
	const options = readPinnedTsConfig();
	const program = ts.createProgram({
		rootNames: [definingModulePath, toolsIndexPath, packageIndexPath, highlightDeclarationPath],
		options,
	});
	const definingFile = sourceFileAt(program, definingModulePath, "truncate module");
	const toolsFile = sourceFileAt(program, toolsIndexPath, "tools index");
	const packageFile = sourceFileAt(program, packageIndexPath, "coding-agent package index");
	assertNoSourceDiagnostics(program, definingFile, "truncate module");
	assertNoSourceDiagnostics(program, toolsFile, "tools index");
	assertNoSourceDiagnostics(program, packageFile, "coding-agent package index");
	const checker = program.getTypeChecker();
	const directLines = resolveDirectConstant(checker, definingFile, "DEFAULT_MAX_LINES");
	const directBytes = resolveDirectConstant(checker, definingFile, "DEFAULT_MAX_BYTES");
	const directGrep = resolveDirectConstant(checker, definingFile, "GREP_MAX_LINE_LENGTH");
	assertIndexAlias(checker, toolsFile, definingFile, directLines, "DEFAULT_MAX_LINES", "tools index");
	assertIndexAlias(checker, toolsFile, definingFile, directBytes, "DEFAULT_MAX_BYTES", "tools index");
	assertIndexAlias(checker, packageFile, definingFile, directLines, "DEFAULT_MAX_LINES", "package index");
	assertIndexAlias(checker, packageFile, definingFile, directBytes, "DEFAULT_MAX_BYTES", "package index");
	assertAbsent(checker, toolsFile, "GREP_MAX_LINE_LENGTH", "tools index");
	assertAbsent(checker, packageFile, "GREP_MAX_LINE_LENGTH", "package index");
	assertExactType(
		checker,
		directLines,
		definingFile,
		ts.TypeFlags.NumberLiteral,
		"2000",
		2000,
		"DEFAULT_MAX_LINES",
	);
	assertExactType(
		checker,
		directBytes,
		definingFile,
		ts.TypeFlags.Number,
		"number",
		undefined,
		"DEFAULT_MAX_BYTES",
	);
	assertExactType(
		checker,
		directGrep,
		definingFile,
		ts.TypeFlags.NumberLiteral,
		"500",
		500,
		"GREP_MAX_LINE_LENGTH",
	);
	proveValidAssignments(options);
	const invalidDiagnostics = proveInvalidDiagnostics(options);
	const runtime = proveRuntimeImports();
	console.log(`compiler=${ts.version}`);
	console.log("const declarations: truncate.ts DEFAULT_MAX_LINES, DEFAULT_MAX_BYTES, GREP_MAX_LINE_LENGTH");
	console.log("types: DEFAULT_MAX_LINES=2000 literal; DEFAULT_MAX_BYTES=number; GREP_MAX_LINE_LENGTH=500 literal");
	console.log("aliases: tools/index.ts and package index -> truncate.ts (DEFAULT_MAX_LINES, DEFAULT_MAX_BYTES only)");
	console.log("GREP_MAX_LINE_LENGTH exports: direct module only");
	console.log("valid assignments: direct/tools/package defaults plus direct GREP");
	console.log(`exact invalid diagnostics: ${invalidDiagnostics} (wrong literals, broad-number string, forbidden GREP imports)`);
	console.log(`runtime imports (direct module and tools index): ${JSON.stringify(runtime)}`);
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`coding-agent truncation constants typeproof failed: ${message}\n`);
	process.exitCode = 1;
}
