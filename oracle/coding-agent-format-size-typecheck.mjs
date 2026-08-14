import { createRequire } from "node:module";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const sourceWorktree = sourceLock.worktree;
const definingModulePath = join(sourceWorktree, "packages/coding-agent/src/core/tools/truncate.ts");
const toolsIndexPath = join(sourceWorktree, "packages/coding-agent/src/core/tools/index.ts");
const packageIndexPath = join(sourceWorktree, "packages/coding-agent/src/index.ts");
const definingImportPath = definingModulePath.endsWith(".ts")
	? definingModulePath.slice(0, -3)
	: definingModulePath;
const toolsImportPath = toolsIndexPath.endsWith(".ts") ? toolsIndexPath.slice(0, -3) : toolsIndexPath;
const packageImportPath = packageIndexPath.endsWith(".ts") ? packageIndexPath.slice(0, -3) : packageIndexPath;
const highlightDeclarationPath = join(
	sourceWorktree,
	"packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const typescriptPath = join(sourceWorktree, "node_modules/typescript/lib/typescript.js");
const require = createRequire(import.meta.url);
const ts = require(typescriptPath);

const validFixtureSource = `
import { formatSize as directFormatSize } from ${JSON.stringify(definingImportPath)};
import { formatSize as toolsFormatSize } from ${JSON.stringify(toolsImportPath)};
import { formatSize as packageFormatSize } from ${JSON.stringify(packageImportPath)};

const directAssignment: string = directFormatSize(0);
const toolsAssignment: string = toolsFormatSize(1024);
const packageAssignment: string = packageFormatSize(1048576);
void directAssignment;
void toolsAssignment;
void packageAssignment;
`;

const invalidFixtureSource = `
import { formatSize as directFormatSize } from ${JSON.stringify(definingImportPath)};
import { formatSize as toolsFormatSize } from ${JSON.stringify(toolsImportPath)};
import { formatSize as packageFormatSize } from ${JSON.stringify(packageImportPath)};

const directWrong = directFormatSize("wrong");
const toolsWrong = toolsFormatSize("wrong");
const packageWrong = packageFormatSize("wrong");
void directWrong;
void toolsWrong;
void packageWrong;
`;

const wrongStringExpectations = [
	{ label: "direct", code: 2345 },
	{ label: "tools", code: 2345 },
	{ label: "package", code: 2345 },
];

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

function resolveDirectFormatSize(checker, definingFile) {
	const exports = moduleExports(checker, definingFile, "truncate module");
	const symbol = exports.find((candidate) => candidate.name === "formatSize");
	if (symbol === undefined) throw new Error("truncate.ts does not export formatSize");
	if ((symbol.flags & ts.SymbolFlags.Alias) !== 0) {
		throw new Error("truncate.ts formatSize must be the defining symbol, not an alias");
	}
	const declarations = symbol.declarations ?? [];
	if (
		declarations.length !== 1 ||
		declarations[0].getSourceFile() !== definingFile ||
		declarations[0].kind !== ts.SyntaxKind.FunctionDeclaration
	) {
		throw new Error("formatSize does not resolve to the real truncate.ts function declaration");
	}
	return symbol;
}

function assertIndexAlias(checker, indexFile, definingFile, definingSymbol, label) {
	const exports = moduleExports(checker, indexFile, label);
	const exportedSymbol = exports.find((candidate) => candidate.name === "formatSize");
	if (exportedSymbol === undefined) throw new Error(`${label} does not export formatSize`);
	if ((exportedSymbol.flags & ts.SymbolFlags.Alias) === 0) {
		throw new Error(`${label} formatSize is not a real index alias`);
	}
	const declarations = exportedSymbol.declarations ?? [];
	if (declarations.length === 0 || declarations.some((declaration) => declaration.getSourceFile() !== indexFile)) {
		throw new Error(`${label} formatSize has a declaration outside the real index`);
	}
	const target = checker.getAliasedSymbol(exportedSymbol);
	const targetDeclarations = target.declarations ?? [];
	if (
		target !== definingSymbol ||
		targetDeclarations.length !== 1 ||
		targetDeclarations[0].getSourceFile() !== definingFile
	) {
		throw new Error(`${label} formatSize does not alias the real truncate.ts declaration`);
	}
	return exportedSymbol;
}

function assertPrimitiveType(checker, type, expectedFlag, expectedName, label) {
	if (type.flags !== expectedFlag || checker.typeToString(type) !== expectedName) {
		throw new Error(`${label} must be exactly ${expectedName}`);
	}
}

function assertFormatSizeSignature(checker, symbol, sourceFile, label) {
	const functionType = checker.getTypeOfSymbolAtLocation(symbol, sourceFile);
	const signatures = checker.getSignaturesOfType(functionType, ts.SignatureKind.Call);
	if (signatures.length !== 1) throw new Error(`${label} must have exactly one call signature`);
	const [signature] = signatures;
	if (signature.parameters.length !== 1 || signature.minArgumentCount !== 1) {
		throw new Error(`${label} must have one required parameter`);
	}
	const [parameter] = signature.parameters;
	if ((parameter.flags & ts.SymbolFlags.Optional) !== 0) {
		throw new Error(`${label} parameter must be required`);
	}
	const parameterDeclaration = parameter.valueDeclaration ?? parameter.declarations?.[0];
	if (parameterDeclaration?.dotDotDotToken !== undefined) {
		throw new Error(`${label} parameter must not be rest syntax`);
	}
	const parameterType = checker.getTypeOfSymbolAtLocation(parameter, sourceFile);
	assertPrimitiveType(checker, parameterType, ts.TypeFlags.Number, "number", `${label} parameter`);
	assertPrimitiveType(checker, signature.getReturnType(), ts.TypeFlags.String, "string", `${label} return type`);
}

function createFixture(source, options, prefix) {
	const directory = mkdtempSync(join(tmpdir(), prefix));
	const fixturePath = join(directory, "format-size.ts");
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

function proveValidFixture(options) {
	const fixture = createFixture(validFixtureSource, options, "pi-coding-agent-format-size-valid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "valid formatSize fixture");
		assertNoSourceDiagnostics(fixture.program, sourceFile, "valid formatSize fixture");
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

function fixtureDiagnostics(program, sourceFile) {
	return [
		...program.getSyntacticDiagnostics(sourceFile),
		...program.getSemanticDiagnostics(sourceFile),
	];
}

function assertWrongStringDiagnostic(sourceFile, diagnostic, expected) {
	if (diagnostic.code !== expected.code || diagnostic.category !== ts.DiagnosticCategory.Error) {
		throw new Error(`unexpected ${expected.label} wrong-string diagnostic: code=${diagnostic.code}`);
	}
	if (diagnosticSpan(sourceFile, diagnostic) !== '"wrong"') {
		throw new Error(`${expected.label} diagnostic does not span the wrong string argument`);
	}
}

function proveWrongStringDiagnostics(options) {
	const fixture = createFixture(invalidFixtureSource, options, "pi-coding-agent-format-size-invalid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "invalid formatSize fixture");
		const diagnostics = fixtureDiagnostics(fixture.program, sourceFile);
		if (diagnostics.length !== wrongStringExpectations.length) {
			throw new Error(
				`expected ${wrongStringExpectations.length} wrong-string diagnostics, got ${diagnostics.length}`,
			);
		}
		for (const [index, diagnostic] of diagnostics.entries()) {
			assertWrongStringDiagnostic(sourceFile, diagnostic, wrongStringExpectations[index]);
		}
		return diagnostics.length;
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

try {
	validateSourceLock();
	verifyPinnedSourceHead();
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
	const directSymbol = resolveDirectFormatSize(checker, definingFile);
	const toolsSymbol = assertIndexAlias(checker, toolsFile, definingFile, directSymbol, "tools index");
	const packageSymbol = assertIndexAlias(checker, packageFile, definingFile, directSymbol, "package index");
	assertFormatSizeSignature(checker, directSymbol, definingFile, "direct formatSize");
	assertFormatSizeSignature(checker, toolsSymbol, toolsFile, "tools-index formatSize");
	assertFormatSizeSignature(checker, packageSymbol, packageFile, "package-index formatSize");
	proveValidFixture(options);
	const wrongStringDiagnostics = proveWrongStringDiagnostics(options);
	console.log(`compiler=${ts.version}`);
	console.log("defining function: truncate.ts formatSize");
	console.log("aliases: core/tools/index.ts -> truncate.ts; package index -> truncate.ts");
	console.log("signature: 1 required number parameter -> string");
	console.log("valid integer assignments: 3 (0, 1024, 1048576)");
	console.log(`wrong-string diagnostics: ${wrongStringDiagnostics} (direct/tools/package; code 2345)`);
	console.log("numeric-domain rejection: not claimed for negative, fractional, or nonfinite values");
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`coding-agent formatSize typeproof failed: ${message}\n`);
	process.exitCode = 1;
}
