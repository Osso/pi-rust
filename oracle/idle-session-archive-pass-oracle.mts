import {
	chmodSync,
	closeSync,
	existsSync,
	mkdirSync,
	openSync,
	readdirSync,
	readFileSync,
	rmSync,
	statSync,
	utimesSync,
	writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { zstdCompressSync, zstdDecompressSync } from "node:zlib";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const { archiveIdleSessionTranscripts } = await import(
	pathToFileURL(`${source}/core/idle-session-archive.ts`).href
);
const { writeSessionMetadata, readSessionMetadata } = await import(
	pathToFileURL(`${source}/core/session-control-db.ts`).href
);

interface Request {
	root: string;
	content: string;
	now: number;
	mtime: number;
	kind:
		| "old"
		| "fresh"
		| "cutoff"
		| "current"
		| "resident"
		| "header-resident"
		| "open"
		| "published"
		| "published-indexed"
		| "changed"
		| "corrupt";
}

for await (const line of createInterface({ input: process.stdin })) {
	let descriptor: number | undefined;
	try {
		const request = JSON.parse(line) as Request;
		const agentDir = join(request.root, "agent");
		const directory = join(agentDir, "sessions", "project");
		mkdirSync(directory, { recursive: true });
		const file = join(directory, "case.jsonl");
		const archive = `${file}.zst`;
		rmSync(file, { force: true });
		rmSync(archive, { force: true });
		const content =
			request.kind === "header-resident"
				? request.content.replace('"idle-old"', '"supervisor"')
				: request.content;
		writeFileSync(file, content, { mode: 0o640 });
		chmodSync(file, 0o640);
		utimesSync(file, request.mtime / 1000, request.mtime / 1000);
		const db = join(request.root, "source.db");
		const seed = (path: string, id: string, archivedAt?: string) =>
			writeSessionMetadata(db, {
				sessionPath: path,
				id,
				cwd: "/fixture-project",
				createdAt: "2020-01-01T00:00:00.000Z",
				modifiedAt: "2020-01-01T00:00:00.000Z",
				messageCount: 1,
				firstMessage: "archive fixture",
				archivedAt,
			});
		seed(join(request.root, "schema-only.jsonl"), "schema-only");
		if (request.kind === "resident") seed(file, "supervisor");
		let inputArchive: string | null = null;
		if (
			["published", "published-indexed", "changed", "corrupt"].includes(
				request.kind,
			)
		) {
			const snapshot =
				request.kind === "changed"
					? request.content.slice(0, request.content.indexOf("\n") + 1)
					: request.content;
			const bytes =
				request.kind === "corrupt"
					? Buffer.from("not a zstd frame")
					: zstdCompressSync(snapshot);
			writeFileSync(archive, bytes);
			chmodSync(archive, 0o600);
			utimesSync(archive, 1650000000, 1650000000);
			inputArchive = bytes.toString("base64");
			if (request.kind === "published-indexed" || request.kind === "changed")
				seed(archive, "idle-old", "2020-01-02T00:00:00.000Z");
		}
		if (request.kind === "open") descriptor = openSync(file, "r");
		const result = await archiveIdleSessionTranscripts(db, {
			agentDir,
			now: request.now,
			currentSessionPath: request.kind === "current" ? file : undefined,
		});
		const metadata = readSessionMetadata(
			db,
			existsSync(archive) ? archive : file,
		);
		const status = existsSync(archive) ? statSync(archive) : undefined;
		console.log(
			JSON.stringify({
				ok: true,
				inputArchive,
				result,
				fileExists: existsSync(file),
				archiveExists: existsSync(archive),
				archiveBytes: existsSync(archive)
					? readFileSync(archive).toString("base64")
					: null,
				decoded:
					existsSync(archive) && request.kind !== "corrupt"
						? zstdDecompressSync(readFileSync(archive)).toString("utf8")
						: null,
				metadata: metadata
					? {
							id: metadata.id,
							cwd: metadata.cwd,
							messageCount: metadata.messageCount,
							firstMessage: metadata.firstMessage,
							isArchived: metadata.isArchived,
						}
					: null,
				mode: status ? status.mode & 0o777 : null,
				mtime: status ? status.mtimeMs : null,
				temps: readdirSync(directory).filter((name) =>
					name.includes(".archive-temp-"),
				),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	} finally {
		if (descriptor !== undefined) closeSync(descriptor);
	}
}
