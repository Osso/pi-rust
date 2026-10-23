import { readFileSync, unlinkSync } from "node:fs";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
	if (request.operation === "spill-failure") {
		process.env.TMPDIR = String(request.tmpdir);
		const trace: string[] = [];
		const definition = {
			name: "fixture", label: "Fixture", description: "Fixture", parameters: {},
			execute: async (_id: string, _params: unknown, _signal: unknown, update: Function) => {
				trace.push("before");
				try { update({ content: [{ type: "text", text: "line\n".repeat(2500) }] }); }
				catch (error) { if (!request.catch) throw error; return { content: [{ type: "text", text: "caught" }] }; }
				trace.push("after");
				return { content: [{ type: "text", text: "after" }] };
			},
		};
		const tool = (api.wrapToolDefinition as Function)(definition);
		try { return { result: await tool.execute("call", {}, undefined, () => {}), trace }; }
		catch (error) { return { error: (error as Error).message.replace(/pi-tool-[0-9a-f]{16}\.log/g, "pi-tool-<id>.log"), trace }; }
	}
	const definition = {
		name: "fixture", label: "Fixture", description: "Fixture", parameters: {},
		constrainedSampling: true,
		execute: async () => request.result,
	};
	const tool = (api.wrapToolDefinition as Function)(definition);
	const result = await tool.execute("call", {});
	const path = result.details?.fullOutputPath;
	let spilled;
	if (path && path !== "existing") {
		spilled = readFileSync(path).toString("hex");
		unlinkSync(path);
		result.content[0].text = result.content[0].text.replaceAll(path, "<spill>");
		result.details.fullOutputPath = "<spill>";
	}
	return { result, spilled, constrainedSampling: tool.constrainedSampling ?? null };
}).catch((error: unknown) => { console.error(error); process.exitCode = 1; });
