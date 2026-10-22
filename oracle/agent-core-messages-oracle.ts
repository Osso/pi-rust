import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]!).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		let output: unknown;
		if (input.op === "builders") {
			const created = [
				api.createBranchSummaryMessage(
					input.summary,
					input.id,
					input.timestamp,
				),
				api.createCompactionSummaryMessage(
					input.summary,
					input.tokens,
					input.timestamp,
					input.options,
				),
				api.createCustomMessage(
					input.customType,
					input.content,
					input.display,
					input.details,
					input.timestamp,
				),
			];
			output = {
				created,
				converted: api.convertToLlm(created),
				markers: [
					api.COMPACTION_SUMMARY_PREFIX,
					api.COMPACTION_SUMMARY_SUFFIX,
					api.BRANCH_SUMMARY_PREFIX,
					api.BRANCH_SUMMARY_SUFFIX,
				],
			};
		} else if (input.op === "bash") {
			if (input.numeric) input.message.exitCode = Number(input.numeric);
			output = api.bashExecutionToText(input.message);
		} else output = api.convertToLlm(input.messages);
		console.log(JSON.stringify(output));
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
