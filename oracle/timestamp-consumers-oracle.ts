import { readFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Request = {
	operation: "info" | "import";
	path: string;
	alias: string;
	options: { homeDir: string; sessionDir: string; fallbackCwd: string };
};
type Source = {
	buildSessionInfo(
		path: string,
	): Promise<{ created: Date; modified: Date } | null>;
	importExternalSessionAlias(
		alias: string,
		options: Request["options"],
	): Promise<{ path: string } | undefined>;
};

async function execute(source: Source, request: Request): Promise<unknown> {
	if (request.operation === "info") {
		const info = await source.buildSessionInfo(request.path);
		return info === null
			? null
			: { created: info.created.getTime(), modified: info.modified.getTime() };
	}
	const imported = await source.importExternalSessionAlias(
		request.alias,
		request.options,
	);
	if (imported === undefined)
		throw new Error("Source import did not find fixture");
	const content = await readFile(imported.path, "utf8");
	return content
		.trim()
		.split("\n")
		.slice(1)
		.map((line) => {
			const entry = JSON.parse(line) as {
				timestamp: string;
				message: { timestamp: number };
			};
			return { entry: entry.timestamp, message: entry.message.timestamp };
		});
}

async function main(): Promise<void> {
	const source = (await import(pathToFileURL(process.argv[2]).href)) as Source;
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const output = await execute(source, JSON.parse(line) as Request);
			process.stdout.write(`${JSON.stringify({ ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) })}\n`,
			);
		}
	}
}

void main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
