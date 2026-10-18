import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { name: string; text: string };

type Api = {
	ansiToHtml(text: string): string;
	ansiLinesToHtml(lines: string[]): string;
};

void runOracle((module, request) => {
	const api = module as unknown as Api;
	return requireCases(request).map((value) => {
		const input = value as Case;
		return {
			name: input.name,
			html: api.ansiToHtml(input.text),
			linesHtml: api.ansiLinesToHtml(input.text.split("\u0000")),
		};
	});
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent ansi-to-html oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
