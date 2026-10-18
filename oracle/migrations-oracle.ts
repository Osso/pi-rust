import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type MigrationsApi = {
	runMigrations(cwd: string): {
		migratedAuthProviders: string[];
		deprecationWarnings: string[];
	};
};

type Request = {
	id: RequestId;
	cwd: string;
	roots: string[];
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function toPosixPath(value: string): string {
	return value.split(sep).join("/");
}

function snapshot(root: string): Array<{ path: string; kind: string; contents?: string }> {
	const entries: Array<{ path: string; kind: string; contents?: string }> = [];
	const walk = (dir: string) => {
		let names: string[];
		try {
			// Directory entry order is filesystem-dependent (per-directory hash seeds),
			// so snapshots are compared sorted. All deterministic arrays are returned
			// in source order.
			names = readdirSync(dir).sort();
		} catch {
			return;
		}
		for (const name of names) {
			const fullPath = join(dir, name);
			let stats;
			try {
				stats = statSync(fullPath);
			} catch {
				continue;
			}
			const entryPath = toPosixPath(relative(root, fullPath));
			if (stats.isDirectory()) {
				entries.push({ path: entryPath, kind: "dir" });
				walk(fullPath);
			} else if (stats.isFile()) {
				entries.push({
					path: entryPath,
					kind: "file",
					contents: readFileSync(fullPath).toString("base64"),
				});
			}
		}
	};
	walk(root);
	return entries;
}

function execute(request: Request, api: MigrationsApi): Response {
	try {
		const consoleLog: string[] = [];
		const originalLog = console.log;
		console.log = (...args: unknown[]) => {
			consoleLog.push(args.map((arg) => String(arg)).join(" "));
		};
		try {
			const result = api.runMigrations(request.cwd);
			return {
				id: request.id,
				ok: true,
				output: {
					migratedAuthProviders: result.migratedAuthProviders,
					deprecationWarnings: result.deprecationWarnings,
					consoleLog,
					trees: request.roots.map((root) => ({ root, entries: snapshot(root) })),
				},
			};
		} finally {
			console.log = originalLog;
		}
	} catch (error) {
		return { id: request.id, ok: false, error: normalizeError(error) };
	}
}

function readSourceModulePath(): string {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("migrations oracle requires a source module path");
	}
	return sourceModulePath;
}

async function main(): Promise<void> {
	const api = (await import(pathToFileURL(readSourceModulePath()).href)) as unknown as MigrationsApi;
	const decoder = new TextDecoder("utf-8");
	let buffer = "";
	process.stdin.on("data", (chunk: Buffer) => {
		buffer += decoder.decode(chunk, { stream: true });
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			let id: RequestId = null;
			try {
				const request = JSON.parse(line) as Request;
				id = request.id;
				process.stdout.write(`${JSON.stringify(execute(request, api))}\n`);
			} catch (error) {
				process.stdout.write(`${JSON.stringify({ id, ok: false, error: normalizeError(error) })}\n`);
			}
		}
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`migrations oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
