import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type ArchiveApi =
	typeof import("@pi-oracle-source/coding-agent/src/core/session-archive-storage.ts");
type Operation =
	| "archiveSessionFile"
	| "restoreArchivedSessionFile"
	| "archivePersistedSession"
	| "restoreArchivedSession";
type Request = { id: number; op: Operation; path: string; db: string };
const root = process.argv[2];
const archive: ArchiveApi = await import(
	pathToFileURL(join(root, "session-archive-storage.ts")).href
);

function invoke(request: Request): string {
	switch (request.op) {
		case "archiveSessionFile":
			return archive.archiveSessionFile(request.path);
		case "restoreArchivedSessionFile":
			return archive.restoreArchivedSessionFile(request.path);
		case "archivePersistedSession":
			return archive.archivePersistedSession(request.db, request.path);
		case "restoreArchivedSession":
			return archive.restoreArchivedSession(request.db, request.path);
	}
}

function projectFailure(error: unknown): unknown {
	if (!(error instanceof Error))
		throw new Error("archive fixture expected a native Error");
	const properties = Object.fromEntries(
		Object.entries(error).filter(([key]) => key !== "stack"),
	);
	return {
		ok: false,
		error: { name: error.name, message: error.message, ...properties },
		constructor: error.constructor.name,
	};
}

for await (const line of createInterface({ input: process.stdin })) {
	const request: Request = JSON.parse(line);
	let output: unknown;
	try {
		output = { ok: true, path: invoke(request) };
	} catch (error) {
		output = projectFailure(error);
	}
	console.log(JSON.stringify({ id: request.id, ok: true, output }));
}
