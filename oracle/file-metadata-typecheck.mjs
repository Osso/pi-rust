import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourceLockPath = join(repoRoot, "docs/porting/source-lock.json");
const sourceLock = JSON.parse(readFileSync(sourceLockPath, "utf8"));
const sourceWorktree = sourceLock.worktree;
const publicIndexPath = join(sourceWorktree, "packages/agent/src/index.ts");
const tsgoPath = join(sourceWorktree, "node_modules/.bin/tsgo");
const typeRoots = join(sourceWorktree, "node_modules/@types");

const fixtureSource = `
import type { FileInfo, FileKind } from ${JSON.stringify(publicIndexPath)};

type Exact<Actual, Expected> = [Actual] extends [Expected]
	? [Expected] extends [Actual]
		? true
		: false
	: false;
type Assert<Condition extends true> = Condition;

type ExpectedFileKind = "file" | "directory" | "symlink";
type ExpectedFileInfo = {
	name: string;
	path: string;
	kind: ExpectedFileKind;
	size: number;
	mtimeMs: number;
};

type FileKindIsExact = Assert<Exact<FileKind, ExpectedFileKind>>;
type FileInfoIsExact = Assert<Exact<FileInfo, ExpectedFileInfo>>;

let info: FileInfo = {
	name: "name",
	path: "/path",
	kind: "file",
	size: 0,
	mtimeMs: 0,
};

info.name = "updated";
info.path = "updated/path";
info.kind = "directory";
info.size = -0;
info.size = Number.NaN;
info.size = Number.POSITIVE_INFINITY;
info.size = Number.NEGATIVE_INFINITY;
info.mtimeMs = -0;
info.mtimeMs = Number.NaN;
info.mtimeMs = Number.POSITIVE_INFINITY;
info.mtimeMs = Number.NEGATIVE_INFINITY;

void info;
`;

function verifySourceHead() {
	const result = spawnSync("git", ["-C", sourceWorktree, "rev-parse", "HEAD"], {
		encoding: "utf8",
	});
	if (result.error !== undefined) {
		throw new Error(`source HEAD check failed to start git: ${result.error.message}`);
	}
	if (result.status !== 0) {
		throw new Error(`source HEAD check failed: ${result.stderr.trim()}`);
	}
	const actualCommit = result.stdout.trim();
	if (actualCommit !== sourceLock.commit) {
		throw new Error(
			`pinned TypeScript source HEAD mismatch: expected ${sourceLock.commit}, got ${actualCommit}`,
		);
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
		"--resolveJsonModule",
		"--allowImportingTsExtensions",
		"--types",
		"node",
		"--typeRoots",
		typeRoots,
		fixturePath,
	];
}

function runTypecheck(fixturePath) {
	const result = spawnSync(tsgoPath, typecheckArguments(fixturePath), {
		cwd: repoRoot,
		encoding: "utf8",
	});
	if (result.stdout !== "") process.stdout.write(result.stdout);
	if (result.stderr !== "") process.stderr.write(result.stderr);
	if (result.error !== undefined) {
		throw new Error(`pinned tsgo failed to start: ${result.error.message}`);
	}
	return result.status ?? 1;
}

function typecheckFixture() {
	const temporaryDirectory = mkdtempSync(join(tmpdir(), "pi-file-metadata-typecheck-"));
	const fixturePath = join(temporaryDirectory, "file-metadata-types.ts");
	writeFileSync(fixturePath, fixtureSource, "utf8");
	try {
		return runTypecheck(fixturePath);
	} finally {
		rmSync(temporaryDirectory, { recursive: true, force: true });
	}
}

try {
	verifySourceHead();
	process.exitCode = typecheckFixture();
} catch (error) {
	const message = error instanceof Error ? error.message : String(error);
	process.stderr.write(`file metadata typecheck failed: ${message}\n`);
	process.exitCode = 1;
}
