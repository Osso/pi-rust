import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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
			} else if (input.kind === "sessionGetter") {
				const { SessionManager } = await import(
					new URL("../../../src/core/session-manager.ts", runtimeUrl).href
				);
				const { MultiAgentStore } = await import(
					new URL("../../../src/core/multi-agent-store.ts", runtimeUrl).href
				);
				const { readMultiAgentState } = await import(
					new URL("../../../src/core/session-control-db.ts", runtimeUrl).href
				);
				const root = mkdtempSync(join(tmpdir(), "profile-session-getter-"));
				const manager = SessionManager.create(root, join(root, "sessions"));
				manager.setMetadataControlDbPath(join(root, "control.sqlite"));
				const store = new MultiAgentStore();
				store.setPersistenceSessionManager(manager);
				let constructed = false;
				const handler = runtime.createMultiAgentPiRequestHandler({
					store,
					createChildSession: async () => {
						constructed = true;
						return {
							prompt: async () => {
								throw new Error("dispatch must not be reached");
							},
						};
					},
				});
				try {
					await handler(
						{ method: "agents.spawn", params: { prompt: "inspect fixture" } },
						{
							cwd: root,
							modelRegistry: { getAll: () => [] },
							get sessionManager() {
								if (constructed)
									throw new Error(
										"session getter failed after child construction",
									);
								return manager;
							},
						},
					);
					throw new Error("expected session getter failure");
				} catch (error) {
					if (!(error instanceof Error)) throw error;
					const state = readMultiAgentState(
						manager.getMetadataControlDbPath(),
						manager.getSessionFile(),
					);
					process.stdout.write(
						`${JSON.stringify({ ok: false, name: error.name, message: error.message, constructed, agents: state?.agents.length ?? 0 })}\n`,
					);
				} finally {
					handler.dispose();
					rmSync(root, { recursive: true, force: true });
				}
				continue;
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
