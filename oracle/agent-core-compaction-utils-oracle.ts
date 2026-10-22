import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
async function main(): Promise<void> {
	const api = await import(pathToFileURL(process.argv[2]!).href);
	for await (const line of createInterface({ input: process.stdin })) {
		const input = JSON.parse(line);
		if (input.op === "serialize")
			console.log(JSON.stringify(api.serializeConversation(input.messages)));
		else {
			const ops = api.createFileOps();
			for (const message of input.messages)
				api.extractFileOpsFromMessage(message, ops);
			const lists = api.computeFileLists(ops);
			console.log(
				JSON.stringify({
					ops: {
						read: [...ops.read],
						written: [...ops.written],
						edited: [...ops.edited],
					},
					lists,
					formatted: api.formatFileOperations(
						lists.readFiles,
						lists.modifiedFiles,
					),
				}),
			);
		}
	}
}
main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
