import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type PathCase = { kind: "normalize" | "mark"; input: string };

void runOracle((api, request) => {
	const normalizePath = api.normalizePath as (input: string) => string;
	const markPathIgnoredByCloudSync = api.markPathIgnoredByCloudSync as (
		input: string,
	) => void;
	return {
		results: requireCases(request).map((value) => {
			const input = value as PathCase;
			try {
				const output =
					input.kind === "normalize"
						? normalizePath(input.input)
						: markPathIgnoredByCloudSync(input.input);
				return { ok: true, value: output ?? null };
			} catch (error) {
				if (!(error instanceof Error)) throw error;
				return {
					ok: false,
					error: Object.assign(
						{ name: error.name, message: error.message },
						error,
					),
				};
			}
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`paths boundary oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
