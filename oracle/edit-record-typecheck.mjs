import { createRequire } from "node:module";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLock = JSON.parse(readFileSync(join(repoRoot, "docs/porting/source-lock.json"), "utf8"));
const sourceWorktree = sourceLock.worktree;
const editDiffPath = join(sourceWorktree, "packages/agent/src/harness/tools/edit-diff.ts");
const editDiffImportPath = editDiffPath.endsWith(".ts") ? editDiffPath.slice(0, -3) : editDiffPath;
const toolsIndexPath = join(sourceWorktree, "packages/agent/src/harness/tools/index.ts");
const packageIndexPath = join(sourceWorktree, "packages/agent/src/index.ts");
const highlightDeclarationPath = join(
	sourceWorktree,
	"packages/agent/src/utils/highlight-js-lib-index.d.ts",
);
const tsconfigPath = join(sourceWorktree, "tsconfig.json");
const typescriptPath = join(sourceWorktree, "node_modules/typescript/lib/typescript.js");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceWorktree, "node_modules/@types");
const require = createRequire(import.meta.url);
const ts = require(typescriptPath);

const validFixtureSource = `
import type { AppliedEditsResult, Edit, FuzzyMatchResult } from ${JSON.stringify(editDiffImportPath)};

const empty: Edit = { oldText: "", newText: "" };
const populated: Edit = { oldText: "before", newText: "after" };
populated.oldText = "updated\\n旧\\0";
populated.newText = "🙂\\r\\n";
const appliedEmpty: AppliedEditsResult = { baseContent: "", newContent: "" };
const appliedPopulated: AppliedEditsResult = { baseContent: "before", newContent: "after" };
appliedPopulated.baseContent = "updated\\n旧\\0";
appliedPopulated.newContent = "🙂\\r\\n";
const fuzzyEmpty: FuzzyMatchResult = {
	found: false,
	index: 0,
	matchLength: 0,
	usedFuzzyMatch: false,
	contentForReplacement: "",
};
const fuzzyPopulated: FuzzyMatchResult = {
	found: true,
	index: 1,
	matchLength: 6,
	usedFuzzyMatch: true,
	contentForReplacement: "before",
};
fuzzyPopulated.found = false;
fuzzyPopulated.index = 2;
fuzzyPopulated.matchLength = 7;
fuzzyPopulated.usedFuzzyMatch = false;
fuzzyPopulated.contentForReplacement = "updated";
void empty;
void populated;
void appliedEmpty;
void appliedPopulated;
`;

const invalidFixtureSource = `
import type { AppliedEditsResult, Edit, FuzzyMatchResult } from ${JSON.stringify(editDiffImportPath)};

const missingOld: Edit = { newText: "new" };
const missingNew: Edit = { oldText: "old" };
const wrongOldType: Edit = { oldText: 42, newText: "new" };
const excessProperty: Edit = { oldText: "old", newText: "new", extra: true };
const missingBaseContent: AppliedEditsResult = { newContent: "new" };
const missingNewContent: AppliedEditsResult = { baseContent: "old" };
const wrongBaseContentType: AppliedEditsResult = { baseContent: 42, newContent: "new" };
const appliedExcessProperty: AppliedEditsResult = { baseContent: "old", newContent: "new", extra: true };
const missingFound: FuzzyMatchResult = {
	index: 0,
	matchLength: 0,
	usedFuzzyMatch: false,
	contentForReplacement: "",
};
const missingIndex: FuzzyMatchResult = {
	found: false,
	matchLength: 0,
	usedFuzzyMatch: false,
	contentForReplacement: "",
};
const missingMatchLength: FuzzyMatchResult = {
	found: false,
	index: 0,
	usedFuzzyMatch: false,
	contentForReplacement: "",
};
const missingUsedFuzzyMatch: FuzzyMatchResult = {
	found: false,
	index: 0,
	matchLength: 0,
	contentForReplacement: "",
};
const missingContentForReplacement: FuzzyMatchResult = {
	found: false,
	index: 0,
	matchLength: 0,
	usedFuzzyMatch: false,
};
const wrongFoundType: FuzzyMatchResult = {
	found: "yes",
	index: 0,
	matchLength: 0,
	usedFuzzyMatch: false,
	contentForReplacement: "",
};
const wrongIndexType: FuzzyMatchResult = {
	found: false,
	index: "zero",
	matchLength: 0,
	usedFuzzyMatch: false,
	contentForReplacement: "",
};
const wrongContentForReplacementType: FuzzyMatchResult = {
	found: false,
	index: 0,
	matchLength: 0,
	usedFuzzyMatch: false,
	contentForReplacement: 42,
};
const fuzzyExcessProperty: FuzzyMatchResult = {
	found: false,
	index: 0,
	matchLength: 0,
	usedFuzzyMatch: false,
	contentForReplacement: "",
	extra: true,
};
void missingOld;
void missingNew;
void wrongOldType;
void excessProperty;
void missingBaseContent;
void missingNewContent;
void wrongBaseContentType;
void appliedExcessProperty;
void missingFound;
void missingIndex;
void missingMatchLength;
void missingUsedFuzzyMatch;
void missingContentForReplacement;
void wrongFoundType;
void wrongIndexType;
void wrongContentForReplacementType;
void fuzzyExcessProperty;
`;

function formatDiagnostics(diagnostics) {
	return ts.formatDiagnosticsWithColorAndContext(diagnostics, {
		getCanonicalFileName: (fileName) => fileName,
		getCurrentDirectory: () => sourceWorktree,
		getNewLine: () => "\\n",
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

function readPinnedCompilerOptions() {
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

function moduleExports(checker, sourceFile, label) {
	const moduleSymbol = checker.getSymbolAtLocation(sourceFile);
	if (moduleSymbol === undefined) throw new Error(`could not obtain ${label} module symbol`);
	return checker.getExportsOfModule(moduleSymbol);
}

function assertSourceDiagnosticsAbsent(program, sourceFile, label) {
	const diagnostics = [
		...program.getSyntacticDiagnostics(sourceFile),
		...program.getSemanticDiagnostics(sourceFile),
	];
	if (diagnostics.length > 0) {
		throw new Error(`${label} diagnostics failed:\n${formatDiagnostics(diagnostics)}`);
	}
}

function resolveEditTarget(checker, editModuleFile) {
	const exports = moduleExports(checker, editModuleFile, "edit-diff module");
	const editSymbol = exports.find((symbol) => symbol.name === "Edit");
	if (editSymbol === undefined) throw new Error("edit-diff.ts does not export Edit");
	const declarations = editSymbol.declarations ?? [];
	if (declarations.length !== 1 || declarations[0].getSourceFile() !== editModuleFile) {
		throw new Error("Edit declaration does not belong to the real edit-diff module");
	}
	if (declarations[0].kind !== ts.SyntaxKind.InterfaceDeclaration) {
		throw new Error("Edit is not the real interface declaration");
	}
	return editSymbol;
}

function resolveAppliedEditsResultTarget(checker, editModuleFile) {
	const exports = moduleExports(checker, editModuleFile, "edit-diff module");
	const appliedSymbol = exports.find((symbol) => symbol.name === "AppliedEditsResult");
	if (appliedSymbol === undefined) throw new Error("edit-diff.ts does not export AppliedEditsResult");
	const declarations = appliedSymbol.declarations ?? [];
	if (declarations.length !== 1 || declarations[0].getSourceFile() !== editModuleFile) {
		throw new Error("AppliedEditsResult declaration does not belong to the real edit-diff module");
	}
	if (declarations[0].kind !== ts.SyntaxKind.InterfaceDeclaration) {
		throw new Error("AppliedEditsResult is not the real interface declaration");
	}
	return appliedSymbol;
}

function assertRequiredWritableString(checker, property, sourceFile, label) {
	if ((property.flags & ts.SymbolFlags.Optional) !== 0) throw new Error(`${label} must be required`);
	const declaration = property.valueDeclaration ?? property.declarations?.[0];
	const modifiers = declaration?.modifiers ?? [];
	if (modifiers.some((modifier) => modifier.kind === ts.SyntaxKind.ReadonlyKeyword)) {
		throw new Error(`${label} must be writable`);
	}
	const type = checker.getTypeOfSymbolAtLocation(property, sourceFile);
	if ((type.flags & ts.TypeFlags.String) === 0) throw new Error(`${label} must be string`);
}

function resolveFuzzyMatchResultTarget(checker, editModuleFile) {
	const exports = moduleExports(checker, editModuleFile, "edit-diff module");
	const fuzzySymbol = exports.find((symbol) => symbol.name === "FuzzyMatchResult");
	if (fuzzySymbol === undefined) throw new Error("edit-diff.ts does not export FuzzyMatchResult");
	const declarations = fuzzySymbol.declarations ?? [];
	if (declarations.length !== 1 || declarations[0].getSourceFile() !== editModuleFile) {
		throw new Error("FuzzyMatchResult declaration does not belong to the real edit-diff module");
	}
	if (declarations[0].kind !== ts.SyntaxKind.InterfaceDeclaration) {
		throw new Error("FuzzyMatchResult is not the real interface declaration");
	}
	return fuzzySymbol;
}

function assertRequiredWritableType(checker, property, sourceFile, expectedTypeFlag, expectedTypeText, label) {
	if ((property.flags & ts.SymbolFlags.Optional) !== 0) throw new Error(`${label} must be required`);
	const declaration = property.valueDeclaration ?? property.declarations?.[0];
	const modifiers = declaration?.modifiers ?? [];
	if (modifiers.some((modifier) => modifier.kind === ts.SyntaxKind.ReadonlyKeyword)) {
		throw new Error(`${label} must be writable`);
	}
	const type = checker.getTypeOfSymbolAtLocation(property, sourceFile);
	if ((type.flags & expectedTypeFlag) === 0 || checker.typeToString(type) !== expectedTypeText) {
		throw new Error(`${label} has the wrong type`);
	}
}

function assertRecordFields(checker, symbol, sourceFile, expected, label) {
	const properties = checker.getDeclaredTypeOfSymbol(symbol).getProperties();
	const names = properties.map((property) => property.name);
	if (names.join("\\0") !== expected.join("\\0")) {
		throw new Error(`${label} properties differ: ${names.join(",")}`);
	}
	for (const [index, name] of expected.entries()) {
		assertRequiredWritableString(checker, properties[index], sourceFile, `${label}.${name}`);
	}
}

function assertFuzzyMatchResultFields(checker, symbol, sourceFile) {
	const properties = checker.getDeclaredTypeOfSymbol(symbol).getProperties();
	const expectedNames = ["found", "index", "matchLength", "usedFuzzyMatch", "contentForReplacement"];
	const names = properties.map((property) => property.name);
	if (names.join("\\0") !== expectedNames.join("\\0")) {
		throw new Error(`FuzzyMatchResult properties differ: ${names.join(",")}`);
	}
	const expectedTypes = [
		{ flag: ts.TypeFlags.BooleanLike, text: "boolean" },
		{ flag: ts.TypeFlags.NumberLike, text: "number" },
		{ flag: ts.TypeFlags.NumberLike, text: "number" },
		{ flag: ts.TypeFlags.BooleanLike, text: "boolean" },
		{ flag: ts.TypeFlags.StringLike, text: "string" },
	];
	for (const [index, expectedType] of expectedTypes.entries()) {
		assertRequiredWritableType(
			checker,
			properties[index],
			sourceFile,
			expectedType.flag,
			expectedType.text,
			`FuzzyMatchResult.${expectedNames[index]}`,
		);
	}
}

function proveExports(checker, editModuleFile, toolsIndexFile, packageIndexFile) {
	const editSymbol = resolveEditTarget(checker, editModuleFile);
	const appliedSymbol = resolveAppliedEditsResultTarget(checker, editModuleFile);
	const fuzzySymbol = resolveFuzzyMatchResultTarget(checker, editModuleFile);
	assertRecordFields(checker, editSymbol, editModuleFile, ["oldText", "newText"], "Edit");
	assertRecordFields(checker, appliedSymbol, editModuleFile, ["baseContent", "newContent"], "AppliedEditsResult");
	assertFuzzyMatchResultFields(checker, fuzzySymbol, editModuleFile);
	const toolsExports = moduleExports(checker, toolsIndexFile, "harness/tools index");
	const packageExports = moduleExports(checker, packageIndexFile, "agent package index");
	for (const name of ["Edit", "AppliedEditsResult", "FuzzyMatchResult"]) {
		if (toolsExports.some((symbol) => symbol.name === name)) {
			throw new Error(`harness/tools index must not export ${name}`);
		}
		if (packageExports.some((symbol) => symbol.name === name)) {
			throw new Error(`agent package index must not export ${name}`);
		}
	}
}

function typecheckArguments(fixturePath) {
	return [
		"--noEmit",
		"--strict",
		"--skipLibCheck",
		"--target",
		"ES2022",
		"--module",
		"NodeNext",
		"--moduleResolution",
		"NodeNext",
		"--allowImportingTsExtensions",
		"--types",
		"node",
		"--typeRoots",
		typeRoots,
		fixturePath,
		highlightDeclarationPath,
	];
}

function runValidFixture(fixturePath) {
	const result = spawnSync(tsgoPath, typecheckArguments(fixturePath), {
		cwd: repoRoot,
		encoding: "utf8",
	});
	if (result.stdout !== "") process.stdout.write(result.stdout);
	if (result.stderr !== "") process.stderr.write(result.stderr);
	if (result.error !== undefined) throw new Error(`tsgo failed to start: ${result.error.message}`);
	if (result.status !== 0) throw new Error(`valid record fixture failed with status ${result.status}`);
}

function createProgramForFixture(source, options, prefix) {
	const directory = mkdtempSync(join(tmpdir(), prefix));
	const fixturePath = join(directory, "edit-record.ts");
	writeFileSync(fixturePath, source, "utf8");
	const program = ts.createProgram({
		rootNames: [fixturePath, editDiffPath, highlightDeclarationPath],
		options: { ...options, noEmit: true },
	});
	return { directory, fixturePath, program };
}

function diagnosticSpan(sourceFile, diagnostic) {
	if (diagnostic.start === undefined || diagnostic.length === undefined) return "";
	return sourceFile.text.slice(diagnostic.start, diagnostic.start + diagnostic.length);
}

function invalidDiagnosticExpectations() {
	return [
		{ code: 2741, text: "missingOld" },
		{ code: 2741, text: "missingNew" },
		{ code: 2322, text: "oldText" },
		{ code: 2353, text: "extra" },
		{ code: 2741, text: "missingBaseContent" },
		{ code: 2741, text: "missingNewContent" },
		{ code: 2322, text: "baseContent" },
		{ code: 2353, text: "extra" },
		{ code: 2741, text: "missingFound" },
		{ code: 2741, text: "missingIndex" },
		{ code: 2741, text: "missingMatchLength" },
		{ code: 2741, text: "missingUsedFuzzyMatch" },
		{ code: 2741, text: "missingContentForReplacement" },
		{ code: 2322, text: "found" },
		{ code: 2322, text: "index" },
		{ code: 2322, text: "contentForReplacement" },
		{ code: 2353, text: "extra" },
	];
}

function assertInvalidDiagnostics(program, sourceFile) {
	const diagnostics = [
		...program.getSyntacticDiagnostics(sourceFile),
		...program.getSemanticDiagnostics(sourceFile),
	];
	const expected = invalidDiagnosticExpectations();
	if (diagnostics.length !== expected.length) {
		throw new Error(`expected ${expected.length} invalid-fixture diagnostics, got ${diagnostics.length}`);
	}
	for (const [index, diagnostic] of diagnostics.entries()) {
		const expectedDiagnostic = expected[index];
		if (diagnostic.code !== expectedDiagnostic.code || diagnostic.category !== ts.DiagnosticCategory.Error) {
			throw new Error(`unexpected invalid-fixture diagnostic at ${index}: code=${diagnostic.code}`);
		}
		if (diagnosticSpan(sourceFile, diagnostic) !== expectedDiagnostic.text) {
			throw new Error(
				`invalid-fixture diagnostic span mismatch at ${index}: ${JSON.stringify(diagnosticSpan(sourceFile, diagnostic))}`,
			);
		}
	}
}

function proveInvalidFixture(options) {
	const fixture = createProgramForFixture(invalidFixtureSource, options, "pi-edit-record-invalid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "invalid record fixture");
		assertInvalidDiagnostics(fixture.program, sourceFile);
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

function proveValidFixture(options) {
	const fixture = createProgramForFixture(validFixtureSource, options, "pi-edit-record-valid-");
	try {
		const sourceFile = sourceFileAt(fixture.program, fixture.fixturePath, "valid record fixture");
		assertSourceDiagnosticsAbsent(fixture.program, sourceFile, "valid record fixture");
		runValidFixture(fixture.fixturePath);
	} finally {
		rmSync(fixture.directory, { recursive: true, force: true });
	}
}

try {
	validateSourceLock();
	verifyPinnedSourceHead();
	const options = readPinnedCompilerOptions();
	const program = ts.createProgram({
		rootNames: [editDiffPath, toolsIndexPath, packageIndexPath, highlightDeclarationPath],
		options,
	});
	const editModuleFile = sourceFileAt(program, editDiffPath, "edit-diff module");
	const toolsIndexFile = sourceFileAt(program, toolsIndexPath, "harness/tools index");
	const packageIndexFile = sourceFileAt(program, packageIndexPath, "agent package index");
	assertSourceDiagnosticsAbsent(program, editModuleFile, "edit-diff module");
	proveExports(program.getTypeChecker(), editModuleFile, toolsIndexFile, packageIndexFile);
	proveValidFixture(options);
	proveInvalidFixture(options);
	console.log("Edit fixtures: 2 valid direct-module assignments, 4 invalid diagnostics");
	console.log("AppliedEditsResult fixtures: 2 valid direct-module assignments, 4 invalid diagnostics");
	console.log("FuzzyMatchResult fixtures: 2 valid direct-module assignments, 9 invalid diagnostics");
	console.log("Edit export: direct edit-diff module only");
	console.log("Edit properties: oldText, newText; required, writable strings");
	console.log("AppliedEditsResult properties: baseContent, newContent; required, writable strings");
	console.log("FuzzyMatchResult properties: found, index, matchLength, usedFuzzyMatch, contentForReplacement; required, writable boolean/number/number/boolean/string");
	console.log("Edit package boundaries: harness/tools index and agent package index omit Edit and AppliedEditsResult");
	console.log("FuzzyMatchResult package boundaries: harness/tools index and agent package index omit FuzzyMatchResult");
	console.log("Record runtime interface identity: not claimed; source-shaped records only");
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`edit record typeproof failed: ${message}\n`);
	process.exitCode = 1;
}
