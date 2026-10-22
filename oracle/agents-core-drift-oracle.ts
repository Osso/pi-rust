import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

async function main() {
	const runtimeUrl = pathToFileURL(process.argv[2]);
	const runtime = await import(runtimeUrl.href);
	const { SettingsManager } = await import(
		new URL("../../../src/core/settings-manager.ts", runtimeUrl).href
	);

	for await (const line of createInterface({
		input: process.stdin,
		terminal: false,
	})) {
		const input = JSON.parse(line);
		try {
			if (input.kind === "profile") {
				const handler = runtime.createMultiAgentPiRequestHandler({
					createChildSession: async () => {
						throw new Error("child creation must not be reached");
					},
				});
				try {
					await handler(
						{
							method: "agents.spawn",
							params: { agentType: "bad", prompt: "inspect fixture" },
						},
						{
							cwd: "/fixture",
							settingsManager: SettingsManager.inMemory({
								agents: { bad: { model: 1 } },
							}),
							modelRegistry: { getAll: () => [] },
						},
					);
				} finally {
					handler.dispose();
				}
			} else if (input.kind === "registration") {
				runtime.registerAgentsCoreTools(
					{
						on: () => {},
						registerCommand: () => {},
						registerTool: (tool: { name: string }) => {
							if (tool.name === "list_agents")
								throw new Error("late registration failed");
						},
					},
					{},
				);
			} else {
				throw new Error("unknown drift fixture");
			}
			process.stdout.write(`${JSON.stringify({ ok: true })}\n`);
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			process.stdout.write(
				`${JSON.stringify({ ok: false, name: error.name, message: error.message })}\n`,
			);
		}
	}
}

main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
