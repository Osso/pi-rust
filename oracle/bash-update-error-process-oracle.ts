import { pathToFileURL } from "node:url";

const { createBashTool } = await import(pathToFileURL(process.argv[2]).href);
const tool = createBashTool(process.cwd(), {
	operations: {
		exec: async (_command: string, _cwd: string, options: { onData: (data: Buffer) => void }) => {
			options.onData(Buffer.from("first\n"));
			options.onData(Buffer.from("second\n"));
			await new Promise((resolve) => setTimeout(resolve, 400));
			return { exitCode: 0 };
		},
	},
});
try {
	await tool.execute("call", { command: "fixture" }, undefined, (result: { content: { text?: string }[] }) => {
		if (result.content.some((entry) => entry.text?.includes("second"))) throw new Error("timer update failed");
	});
	console.log("completed");
} catch { console.log("caught by execute"); }
