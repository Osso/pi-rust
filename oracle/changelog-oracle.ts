// Oracle for `utils/changelog.ts`: link normalization, entry parsing, comparisons, and filtering.
import { serve } from "./config-auth-oracle-support.ts";

type ChangelogApi = {
	normalizeChangelogLinks(markdown: string, version: string | { major: number; minor: number; patch: number; content: string }): string;
	parseChangelog(path: string): Array<{ major: number; minor: number; patch: number; content: string }>;
	compareVersions(
		left: { major: number; minor: number; patch: number; content: string },
		right: { major: number; minor: number; patch: number; content: string },
	): number;
	getNewEntries(
		entries: Array<{ major: number; minor: number; patch: number; content: string }>,
		lastVersion: string,
	): Array<{ major: number; minor: number; patch: number; content: string }>;
};

async function main(): Promise<void> {
	const { pathToFileURL } = await import("node:url");
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("changelog oracle requires a source module path");
	const api = (await import(pathToFileURL(sourcePath).href)) as ChangelogApi;

	serve((request) => {
		if (request.op === "links") {
			return { markdown: api.normalizeChangelogLinks(request.markdown as string, request.version as string) };
		}
		if (request.op === "parse") {
			return { entries: api.parseChangelog(request.path as string) };
		}
		const entries = (request.entries as Array<{ major: number; minor: number; patch: number; content: string }>) ?? [];
		if (request.op === "compare") {
			const left = request.left as { major: number; minor: number; patch: number; content: string };
			const right = request.right as { major: number; minor: number; patch: number; content: string };
			return { comparison: Math.sign(api.compareVersions(left, right)) };
		}
		return { entries: api.getNewEntries(entries, request.lastVersion as string) };
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`changelog oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
