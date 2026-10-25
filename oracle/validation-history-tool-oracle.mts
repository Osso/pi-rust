import { dirname, resolve } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("validation source path required");
const validation = await import(pathToFileURL(source).href);
const history = await import(
	pathToFileURL(
		resolve(
			dirname(source),
			"../../../coding-agent/src/core/tools/search-current-session-history.ts",
		),
	).href
);
const tool = history.createSearchCurrentSessionHistoryToolDefinition();
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const results = request.arguments.map((args: Record<string, unknown>) => {
		try {
			return {
				ok: true,
				value: validation.validateToolArguments(tool, {
					type: "toolCall",
					id: "history",
					name: tool.name,
					arguments: args,
				}),
			};
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			return { ok: false, error: { name: error.name, message: error.message } };
		}
	});
	process.stdout.write(
		`${JSON.stringify({ id: request.id, ok: true, results })}\n`,
	);
}
