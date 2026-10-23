import { readFileSync, unlinkSync } from "node:fs";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

void runOracle(async (api, request) => {
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
