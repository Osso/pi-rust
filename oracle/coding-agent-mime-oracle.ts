import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { name: string; bytes: number[] };

void runOracle((api, request) => {
	const detectSupportedImageMimeType = api.detectSupportedImageMimeType as (buffer: Uint8Array) => string | null;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			return {
				name: input.name,
				value: detectSupportedImageMimeType(Uint8Array.from(input.bytes)) ?? null,
			};
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`mime oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
