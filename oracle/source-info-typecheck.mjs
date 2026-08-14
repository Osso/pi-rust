import { createRequire } from "node:module";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const sourceWorktree = sourceLock.worktree;
const sourceInfoPath = join(sourceWorktree, "packages/coding-agent/src/core/source-info.ts");
const publicIndexPath = join(sourceWorktree, "packages/coding-agent/src/index.ts");
const publicIndexImportPath = publicIndexPath.endsWith(".ts") ? publicIndexPath.slice(0, -3) : publicIndexPath;
const highlightDeclarationPath = join(
	sourceWorktree,
	"packages/coding-agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const typescriptPath = join(sourceWorktree, "node_modules/typescript/lib/typescript.js");
const require = createRequire(import.meta.url);
const ts = require(typescriptPath);

const validFixtureSource = `
import { createSyntheticSourceInfo, type SourceInfo } from ${JSON.stringify(publicIndexImportPath)};

const defaults: SourceInfo = createSyntheticSourceInfo("", { source: "" });
const explicit: SourceInfo = createSyntheticSourceInfo("path", {
  source: "source",
  scope: "user",
  origin: "package",
  baseDir: "",
});
const explicitUndefined: SourceInfo = createSyntheticSourceInfo("path", {
  source: "source",
  scope: undefined,
  origin: undefined,
  baseDir: undefined,
});

explicit.path = "updated";
explicit.source = "updated source";
explicit.baseDir = undefined;
void defaults;
void explicit;
void explicitUndefined;
`;

const invalidFixtureSource = `
import { createSyntheticSourceInfo } from ${JSON.stringify(publicIndexImportPath)};

const missingSource = createSyntheticSourceInfo("path", {});
const invalidScope = createSyntheticSourceInfo("path", { source: "source", scope: "invalid" });
const invalidOrigin = createSyntheticSourceInfo("path", { source: "source", origin: "invalid" });
const excessOption = createSyntheticSourceInfo("path", { source: "source", extra: true });
void missingSource;
void invalidScope;
void invalidOrigin;
void excessOption;
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

function verifySourceHead() {
	const result = spawnSync("git", ["-C", sourceWorktree, "rev-parse", "HEAD"], { encoding: "utf8" });
	if (result.error !== undefined) throw new Error(`source HEAD check failed to start git: ${result.error.message}`);
	if (result.status !== 0) throw new Error(`source HEAD check failed: ${result.stderr.trim()}`);
	const actualCommit = result.stdout.trim();
	if (actualCommit !== sourceLock.commit) {
		throw new Error(`pinned source HEAD mismatch: expected ${sourceLock.commit}, got ${actualCommit}`);
	}
}

function readPinnedCompilerOptions() {
	const configFile = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
	if (configFile.error !== undefined) throw new Error(formatDiagnostics([configFile.error]));
	const parsed = ts.parseJsonConfigFileContent(configFile.config, ts.sys, sourceWorktree, undefined, tsconfigPath);
	if (parsed.errors.length > 0) throw new Error(formatDiagnostics(parsed.errors));
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
	if (diagnostics.length > 0) throw new Error(`${label} diagnostics failed:\n${formatDiagnostics(diagnostics)}`);
}

function resolveSymbol(checker, symbol, sourceFile, label) {
	if (symbol === undefined) throw new Error(`${label} is not exported`);
	const target = (symbol.flags & ts.SymbolFlags.Alias) !== 0 ? checker.getAliasedSymbol(symbol) : symbol;
	const declarations = target.declarations ?? [];
	if (declarations.length !== 1 || declarations[0].getSourceFile() !== sourceFile) {
		throw new Error(`${label} does not resolve to the real source-info module`);
	}
	return target;
}

function assertStringUnion(checker, symbol, expected, label) {
	const type = checker.getDeclaredTypeOfSymbol(symbol);
	const members = type.isUnion() ? type.types : [];
	const values = members.map((member) => member.value).sort();
	if (JSON.stringify(values) !== JSON.stringify([...expected].sort())) {
		throw new Error(`${label} literals differ: ${JSON.stringify(values)}`);
	}
}

function assertPropertyFlags(property, item, label) {
	const expectedOptional = item.optional ? ts.SymbolFlags.Optional : 0;
	if ((property.flags & ts.SymbolFlags.Optional) !== expectedOptional) {
		throw new Error(`${label}.${item.name} optionality differs`);
	}
	const declaration = property.valueDeclaration ?? property.declarations?.[0];
	const modifiers = declaration?.modifiers ?? [];
	if (modifiers.some((modifier) => modifier.kind === ts.SyntaxKind.ReadonlyKeyword)) {
		throw new Error(`${label}.${item.name} must be writable`);
	}
}

function assertPropertyType(checker, property, sourceFile, item, label) {
	const propertyType = checker.getTypeOfSymbolAtLocation(property, sourceFile);
	const members = propertyType.isUnion() ? propertyType.types : [propertyType];
	const values = members.filter((member) => (member.flags & ts.TypeFlags.Undefined) === 0);
	if (item.optional && !members.some((member) => (member.flags & ts.TypeFlags.Undefined) !== 0)) {
		throw new Error(`${label}.${item.name} must include undefined`);
	}
	if (item.literals !== undefined) {
		const actual = values.map((member) => member.value).sort();
		if (JSON.stringify(actual) !== JSON.stringify([...item.literals].sort())) {
			throw new Error(`${label}.${item.name} literals differ`);
		}
		return;
	}
	if (values.length !== 1 || (values[0].flags & item.typeFlag) === 0) {
		throw new Error(`${label}.${item.name} has the wrong type`);
	}
}

function assertRecordProperties(checker, type, sourceFile, expected, label) {
	const properties = type.getProperties();
	const names = properties.map((property) => property.name);
	if (JSON.stringify(names) !== JSON.stringify(expected.map((item) => item.name))) {
		throw new Error(`${label} properties differ: ${names.join(",")}`);
	}
	for (const [index, item] of expected.entries()) {
		const property = properties[index];
		assertPropertyFlags(property, item, label);
		assertPropertyType(checker, property, sourceFile, item, label);
	}
}

function assertSyntheticSignature(checker, symbol, sourceFile, sourceInfoSymbol) {
	const functionType = checker.getTypeOfSymbolAtLocation(symbol, sourceFile);
	const signatures = checker.getSignaturesOfType(functionType, ts.SignatureKind.Call);
	if (signatures.length !== 1 || signatures[0].parameters.length !== 2) {
		throw new Error("createSyntheticSourceInfo signature shape differs");
	}
	const [pathParameter, optionsParameter] = signatures[0].parameters;
	if ((checker.getTypeOfSymbolAtLocation(pathParameter, sourceFile).flags & ts.TypeFlags.String) === 0) {
		throw new Error("createSyntheticSourceInfo path must be string");
	}
	const optionsType = checker.getTypeOfSymbolAtLocation(optionsParameter, sourceFile);
	assertRecordProperties(checker, optionsType, sourceFile, [
		{ name: "source", optional: false, typeFlag: ts.TypeFlags.String },
		{ name: "scope", optional: true, literals: ["user", "project", "temporary"] },
		{ name: "origin", optional: true, literals: ["package", "top-level"] },
		{ name: "baseDir", optional: true, typeFlag: ts.TypeFlags.String },
	], "createSyntheticSourceInfo options");
	const returnType = signatures[0].getReturnType();
	const sourceInfoType = checker.getDeclaredTypeOfSymbol(sourceInfoSymbol);
	if (!checker.isTypeAssignableTo(returnType, sourceInfoType) || !checker.isTypeAssignableTo(sourceInfoType, returnType)) {
		throw new Error("synthetic return type is not SourceInfo");
	}
}

function assertPackageBoundaries(checker, packageIndexFile, sourceInfoSymbol, syntheticSymbol, sourceInfoFile) {
	const exports = moduleExports(checker, packageIndexFile, "coding-agent package index");
	const sourceInfoExport = exports.find((symbol) => symbol.name === "SourceInfo");
	const syntheticExport = exports.find((symbol) => symbol.name === "createSyntheticSourceInfo");
	if (sourceInfoExport === undefined || syntheticExport === undefined) throw new Error("package exports are incomplete");
	if (resolveSymbol(checker, sourceInfoExport, sourceInfoFile, "package SourceInfo") !== sourceInfoSymbol) {
		throw new Error("package SourceInfo target differs");
	}
	if (resolveSymbol(checker, syntheticExport, sourceInfoFile, "package createSyntheticSourceInfo") !== syntheticSymbol) {
		throw new Error("package createSyntheticSourceInfo target differs");
	}
	for (const name of ["SourceScope", "SourceOrigin", "createSourceInfo"]) {
		if (exports.some((symbol) => symbol.name === name)) throw new Error(`package index must omit ${name}`);
	}
}

function createFixture(source, options, prefix) {
	const directory = mkdtempSync(join(tmpdir(), prefix));
	const fixturePath = join(directory, "source-info.ts");
	writeFileSync(fixturePath, source, "utf8");
	const program = ts.createProgram({ rootNames: [fixturePath, publicIndexPath, sourceInfoPath, highlightDeclarationPath], options: { ...options, noEmit: true } });
	return { directory, fixturePath, program };
}

function diagnosticSpan(sourceFile, diagnostic) {
	if (diagnostic.start === undefined || diagnostic.length === undefined) return "";
	return sourceFile.text.slice(diagnostic.start, diagnostic.start + diagnostic.length);
}

function assertInvalidDiagnostics(program, sourceFile) {
	const diagnostics = [...program.getSyntacticDiagnostics(sourceFile), ...program.getSemanticDiagnostics(sourceFile)];
	if (diagnostics.length !== 4) throw new Error(`expected 4 invalid diagnostics, got ${diagnostics.length}`);
	const expected = [
		{ code: 2345, span: "{}" },
		{ code: 2322, span: "scope" },
		{ code: 2322, span: "origin" },
		{ code: 2353, span: "extra" },
	];
	for (const [index, diagnostic] of diagnostics.entries()) {
		const expectedDiagnostic = expected[index];
		if (diagnostic.code !== expectedDiagnostic.code || diagnostic.category !== ts.DiagnosticCategory.Error) {
			throw new Error(`unexpected invalid diagnostic ${index}: ${diagnostic.code}`);
		}
		if (diagnosticSpan(sourceFile, diagnostic) !== expectedDiagnostic.span) {
			throw new Error(`invalid diagnostic ${index} span mismatch`);
		}
	}
}

function proveInvalidFixture(options) {
	const fixture = createFixture(invalidFixtureSource, options, "pi-source-info-invalid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "invalid SourceInfo fixture");
		assertInvalidDiagnostics(fixture.program, sourceFile);
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

function proveValidFixture(options) {
	const fixture = createFixture(validFixtureSource, options, "pi-source-info-valid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "valid SourceInfo fixture");
		assertNoSourceDiagnostics(fixture.program, sourceFile, "valid SourceInfo fixture");
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

try {
	validateSourceLock();
	verifySourceHead();
	const options = readPinnedCompilerOptions();
	const program = ts.createProgram({ rootNames: [publicIndexPath, sourceInfoPath, highlightDeclarationPath], options });
	const packageIndexFile = sourceFileAt(program, publicIndexPath, "coding-agent package index");
	const sourceInfoFile = sourceFileAt(program, sourceInfoPath, "source-info module");
	assertNoSourceDiagnostics(program, packageIndexFile, "coding-agent package index");
	assertNoSourceDiagnostics(program, sourceInfoFile, "source-info module");
	const checker = program.getTypeChecker();
	const sourceExports = moduleExports(checker, sourceInfoFile, "source-info module");
	const sourceScope = resolveSymbol(checker, sourceExports.find((symbol) => symbol.name === "SourceScope"), sourceInfoFile, "SourceScope");
	const sourceOrigin = resolveSymbol(checker, sourceExports.find((symbol) => symbol.name === "SourceOrigin"), sourceInfoFile, "SourceOrigin");
	const sourceInfo = resolveSymbol(checker, sourceExports.find((symbol) => symbol.name === "SourceInfo"), sourceInfoFile, "SourceInfo");
	const synthetic = resolveSymbol(checker, sourceExports.find((symbol) => symbol.name === "createSyntheticSourceInfo"), sourceInfoFile, "createSyntheticSourceInfo");
	if (!sourceExports.some((symbol) => symbol.name === "createSourceInfo")) throw new Error("source-info module must export createSourceInfo");
	assertStringUnion(checker, sourceScope, ["user", "project", "temporary"], "SourceScope");
	assertStringUnion(checker, sourceOrigin, ["package", "top-level"], "SourceOrigin");
	assertRecordProperties(checker, checker.getDeclaredTypeOfSymbol(sourceInfo), sourceInfoFile, [
		{ name: "path", optional: false, typeFlag: ts.TypeFlags.String },
		{ name: "source", optional: false, typeFlag: ts.TypeFlags.String },
		{ name: "scope", optional: false, literals: ["user", "project", "temporary"] },
		{ name: "origin", optional: false, literals: ["package", "top-level"] },
		{ name: "baseDir", optional: true, typeFlag: ts.TypeFlags.String },
	], "SourceInfo");
	assertSyntheticSignature(checker, synthetic, sourceInfoFile, sourceInfo);
	assertPackageBoundaries(checker, packageIndexFile, sourceInfo, synthetic, sourceInfoFile);
	proveValidFixture(options);
	proveInvalidFixture(options);
	console.log(`compiler=${ts.version}`);
	console.log("source-info module: SourceScope, SourceOrigin, SourceInfo, createSourceInfo, createSyntheticSourceInfo");
	console.log("package index: SourceInfo and createSyntheticSourceInfo only");
	console.log("SourceInfo fields: path, source, scope, origin; optional writable baseDir");
	console.log("synthetic options: anonymous TypeScript shape; Rust carrier SyntheticSourceInfoOptions approved");
	console.log(`exactOptionalPropertyTypes=${options.exactOptionalPropertyTypes === true}`);
	console.log("invalid diagnostics: 4 structural errors, no suppressed diagnostics");
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`source info typeproof failed: ${message}\n`);
	process.exitCode = 1;
}
