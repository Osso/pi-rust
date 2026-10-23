import { rmdirSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { SessionManager } = await import(
	pathToFileURL(`${root}/core/session-manager.ts`).href
);
const { readProjectSpec } = await import(
	pathToFileURL(`${root}/architect/project-spec.ts`).href
);
const units = (value: string) =>
	Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));

function errorWire(error: Error) {
	const record = error as Error & {
		code?: string;
		errno?: number;
		syscall?: string;
		path?: string;
		dest?: string;
	};
	return {
		ok: false,
		error: {
			name: error.name,
			message: units(error.message),
			code: record.code ?? null,
			errno: record.errno ?? null,
			syscall: record.syscall ?? null,
			path: record.path === undefined ? null : units(record.path),
			dest: record.dest === undefined ? null : units(record.dest),
		},
	};
}

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as {
		directory: string;
		warm: boolean;
		project?: string;
	};
	const original = process.cwd();
	try {
		process.chdir(request.directory);
		if (request.warm) SessionManager.inMemory();
		rmdirSync(request.directory);
		try {
			const output =
				request.project === undefined
					? { ok: true, cwd: units(SessionManager.inMemory().getCwd()) }
					: {
							ok: true,
							text: await readProjectSpec(request.project, "README.md"),
						};
			console.log(JSON.stringify(output));
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			console.log(JSON.stringify(errorWire(error)));
		}
	} finally {
		process.chdir(original);
	}
}
