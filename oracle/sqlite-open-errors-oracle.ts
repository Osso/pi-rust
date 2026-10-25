import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createSqliteDatabase(path: string): { close(): void };
	createReadOnlySqliteDatabase(path: string): { close(): void };
};
type NodeSqliteError = Error & {
	code?: string;
	errcode?: number;
	errstr?: string;
};

void runOracle((module, request) =>
	requireCases(request).map((value) => {
		const input = value as { path: string; readOnly: boolean };
		const api = module as Api;
		try {
			const db = input.readOnly
				? api.createReadOnlySqliteDatabase(input.path)
				: api.createSqliteDatabase(input.path);
			db.close();
			return { ok: true };
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			const sqlite = error as NodeSqliteError;
			return {
				ok: false,
				error: {
					name: sqlite.name,
					message: sqlite.message,
					code: sqlite.code,
					errcode: sqlite.errcode,
					errstr: sqlite.errstr,
				},
			};
		}
	}),
).catch((error: unknown) => {
	process.stderr.write(`sqlite open oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
