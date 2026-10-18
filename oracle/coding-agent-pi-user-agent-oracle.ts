import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { name: string; version: string };

void runOracle((api, request) => {
	const getPiUserAgent = api.getPiUserAgent as (version: string) => string;
	const runtime = process.versions.bun ? `bun/${process.versions.bun}` : `node/${process.version}`;
	return {
		platform: process.platform,
		arch: process.arch,
		runtime,
		results: requireCases(request).map((value) => {
			const input = value as Case;
			return { name: input.name, value: getPiUserAgent(input.version) };
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`pi-user-agent oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
