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

	const checkUiCallback = async (invalidatedAtCompletion: boolean) => {
		const { SessionManager } = await import(
			new URL("../../../src/core/session-manager.ts", runtimeUrl).href
		);
		const { MultiAgentStore } = await import(
			new URL("../../../src/core/multi-agent-store.ts", runtimeUrl).href
		);
		const root = mkdtempSync(join(tmpdir(), "background-ui-lifetime-"));
		const manager = SessionManager.create(root, join(root, "sessions"));
		manager.setMetadataControlDbPath(join(root, "control.sqlite"));
		const store = new MultiAgentStore();
		store.setPersistenceSessionManager(manager);
		const handles = runtime.createMultiAgentRuntimeHandles();
		const notifications: string[] = [];
		const ui = {
			notify: (message: string) => notifications.push(message),
			setEditorText: () => {},
		};
		const { ExtensionRunner } = await import(
			new URL("../../../src/core/extensions/runner.ts", runtimeUrl).href
		);
		const { createExtensionRuntime } = await import(
			new URL("../../../src/core/extensions/loader.ts", runtimeUrl).href
		);
		const runner = new ExtensionRunner(
			[],
			createExtensionRuntime(),
			root,
			manager,
			{ getAll: () => [] },
		);
		runner.setUIContext(ui, "rpc");
		const ctx = runner.createCommandContext();
		const diagnostics: string[] = [];
		const originalError = console.error;
		console.error = (...values: unknown[]) => {
			diagnostics.push(
				values
					.map((value) =>
						value instanceof Error ? value.message : String(value),
					)
					.join(" "),
			);
			originalError(...values);
		};
		const completion = Promise.withResolvers<void>();
		const reported = Promise.withResolvers<unknown>();
		const onRejection = (reason: unknown) => reported.resolve(reason);
		const commands = new Map<
			string,
			{ handler: (args: string, ctx: unknown) => Promise<void> }
		>();
		process.on("unhandledRejection", onRejection);
		try {
			runtime.registerAgentsCoreTools(
				{
					on: () => {},
					registerTool: () => {},
					registerCommand: (
						name: string,
						command: { handler: (args: string, ctx: unknown) => Promise<void> },
					) => commands.set(name, command),
				},
				{
					store,
					runtimeHandles: handles,
					desktopNotifier: () => undefined,
					createChildSession: async () => ({
						messages: [],
						prompt: () => completion.promise,
					}),
				},
			);
			const command = commands.get("bg");
			if (!command) throw new Error("source did not register bg");
			await command.handler("inspect fixture", ctx);
			const before = notifications.length;
			const dispatch = handles.dispatches.values().next().value;
			if (!dispatch)
				throw new Error("source did not start background dispatch");
			if (invalidatedAtCompletion)
				runner.invalidate("ui getter failed after dispatch started");
			completion.resolve();
			const finalAgent = await dispatch;
			await new Promise<void>((resolve) => setImmediate(resolve));
			let failure: string | undefined;
			if (invalidatedAtCompletion) {
				const timer = setTimeout(
					() => reported.reject(new Error("source omitted UI rejection")),
					3000,
				);
				try {
					const error = await reported.promise;
					if (!(error instanceof Error))
						throw new Error("expected source Error rejection");
					failure = error.message;
				} finally {
					clearTimeout(timer);
				}
			}
			return {
				before,
				after: notifications.length,
				lifecycle: finalAgent.lifecycle,
				failure,
				diagnostics,
			};
		} finally {
			completion.resolve();
			await Promise.all([...handles.dispatches.values()]);
			process.off("unhandledRejection", onRejection);
			console.error = originalError;
			rmSync(root, { recursive: true, force: true });
		}
	};

	for await (const line of createInterface({
		input: process.stdin,
		terminal: false,
	})) {
		const input = JSON.parse(line);
		try {
			if (input.kind === "spawnWithoutJournal") {
                const { SessionManager } = await import(new URL("../../../src/core/session-manager.ts", runtimeUrl).href);
                const { MultiAgentStore } = await import(new URL("../../../src/core/multi-agent-store.ts", runtimeUrl).href);
                const root = mkdtempSync(join(tmpdir(), "spawn-without-journal-"));
                const manager = SessionManager.create(root, join(root, "sessions"));
                manager.setMetadataControlDbPath(join(root, "control.sqlite"));
                const store = new MultiAgentStore();
                store.setPersistenceSessionManager(manager);
                const handles = runtime.createMultiAgentRuntimeHandles();
                let finish: (() => void) | undefined;
                const pending = new Promise<void>((resolve) => { finish = resolve; });
                const handler = runtime.createMultiAgentPiRequestHandler({
                    store, runtimeHandles: handles,
                    createChildSession: async () => ({messages: [], prompt: () => pending}),
                });
                try {
                    const result = await handler({method:"agents.spawn",params:{prompt:"work"}}, {
                        cwd:root,sessionManager:manager,modelRegistry:{getAll:()=>[]},
                    });
                    const dispatch = handles.dispatches.get(result.agent.id);
                    if (!dispatch || !finish) throw new Error("source omitted dispatch");
                    finish();
                    const terminal = await dispatch;
                    process.stdout.write(`${JSON.stringify({dispatched:result.dispatched,lifecycle:terminal.lifecycle,hasTranscript:terminal.transcript !== undefined,customEntries:manager.getBranch().filter((entry: {type:string}) => entry.type === "custom").length})}\n`);
                } finally {
                    handler.dispose();
                    rmSync(root,{recursive:true,force:true});
                }
                continue;
            } else if (input.kind === "profile") {
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
			} else if (input.kind === "profileContext") {
				const { SessionManager } = await import(
					new URL("../../../src/core/session-manager.ts", runtimeUrl).href
				);
				const { MultiAgentStore } = await import(
					new URL("../../../src/core/multi-agent-store.ts", runtimeUrl).href
				);
				const root = mkdtempSync(join(tmpdir(), "profile-untouched-tools-"));
				const manager = SessionManager.create(root, join(root, "sessions"));
				manager.setMetadataControlDbPath(join(root, "control.sqlite"));
				const store = new MultiAgentStore();
				store.setPersistenceSessionManager(manager);
				let observedContext: string | undefined;
				const handler = runtime.createMultiAgentPiRequestHandler({
					store,
					createChildSession: async (request: { context: string }) => {
						observedContext = request.context;
						throw new Error("factory observed profile context");
					},
				});
				try {
					const result = await handler(
						{
							method: "agents.spawn",
							params: { agentType: "bad", prompt: "inspect fixture" },
						},
						{
							cwd: root,
							sessionManager: manager,
							modelRegistry: { getAll: () => [] },
							settingsManager: SettingsManager.inMemory({
								agents: { bad: { context: "fresh", tools: 7 } },
							}),
						},
					);
					process.stdout.write(
						`${JSON.stringify({ context: observedContext, dispatched: result.dispatched, lifecycle: result.agent.lifecycle })}\n`,
					);
				} finally {
					handler.dispose();
					rmSync(root, { recursive: true, force: true });
				}
				continue;
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
			} else if (input.kind === "uiCallback") {
				process.stdout.write(
					`${JSON.stringify(await checkUiCallback(input.invalidated))}\n`,
				);
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
