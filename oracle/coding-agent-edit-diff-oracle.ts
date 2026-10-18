import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Edit = { oldText: string; newText: string };

type Case =
	| { kind: "normalize"; text: string }
	| { kind: "fuzzyFind"; content: string; oldText: string }
	| { kind: "applyEdits"; content: string; edits: Edit[]; path: string }
	| { kind: "diffString"; oldContent: string; newContent: string; contextLines: number }
	| { kind: "patch"; path: string; oldContent: string; newContent: string; contextLines: number }
	| { kind: "preview"; path: string; edits: Edit[]; cwd: string };

type Api = {
	normalizeForFuzzyMatch(text: string): string;
	fuzzyFindText(content: string, oldText: string): {
		found: boolean;
		index: number;
		matchLength: number;
		usedFuzzyMatch: boolean;
		contentForReplacement: string;
	};
	applyEditsToNormalizedContent(
		content: string,
		edits: Edit[],
		path: string,
	): { baseContent: string; newContent: string };
	generateDiffString(
		oldContent: string,
		newContent: string,
		contextLines: number,
	): { diff: string; firstChangedLine: number | undefined };
	generateUnifiedPatch(path: string, oldContent: string, newContent: string, contextLines: number): string;
	computeEditsDiff(
		path: string,
		edits: Edit[],
		cwd: string,
	): Promise<{ diff: string; firstChangedLine: number | undefined } | { error: string }>;
};

void runOracle(async (module, request) => {
	const api = module as unknown as Api;
	const results = [];
	for (const value of requireCases(request)) {
		const input = value as Case;
		if (input.kind === "normalize") {
			results.push({ value: api.normalizeForFuzzyMatch(input.text) });
		} else if (input.kind === "fuzzyFind") {
			const found = api.fuzzyFindText(input.content, input.oldText);
			results.push({
				found: found.found,
				index: found.index,
				matchLength: found.matchLength,
				usedFuzzyMatch: found.usedFuzzyMatch,
				contentForReplacement: found.contentForReplacement,
			});
		} else if (input.kind === "applyEdits") {
			try {
				const applied = api.applyEditsToNormalizedContent(input.content, input.edits, input.path);
				results.push({ ok: true, baseContent: applied.baseContent, newContent: applied.newContent });
			} catch (error) {
				results.push({ ok: false, error: error instanceof Error ? error.message : String(error) });
			}
		} else if (input.kind === "diffString") {
			const diff = api.generateDiffString(input.oldContent, input.newContent, input.contextLines);
			results.push({ diff: diff.diff, firstChangedLine: diff.firstChangedLine ?? null });
		} else if (input.kind === "patch") {
			results.push({ patch: api.generateUnifiedPatch(input.path, input.oldContent, input.newContent, input.contextLines) });
		} else {
			const outcome = await api.computeEditsDiff(input.path, input.edits, input.cwd);
			results.push(
				"error" in outcome
					? { error: outcome.error }
					: { diff: outcome.diff, firstChangedLine: outcome.firstChangedLine ?? null },
			);
		}
	}
	return results;
}).catch((error: unknown) => {
	process.stderr.write(`coding-agent edit-diff oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
