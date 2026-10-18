import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = { name: string; content: string };

void runOracle((api, request) => {
	const parseFrontmatter = api.parseFrontmatter as (content: string) => {
		frontmatter: unknown;
		body: string;
	};
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			try {
				const parsed = parseFrontmatter(input.content);
				return { name: input.name, ok: true, value: { frontmatter: parsed.frontmatter, body: parsed.body } };
			} catch {
				return { name: input.name, ok: false };
			}
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`frontmatter oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
