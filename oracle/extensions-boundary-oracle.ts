import { createInterface } from "node:readline";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

async function main(): Promise<void> {
	const core = join(dirname(process.argv[2]), "..");
	const load = (name: string) => import(pathToFileURL(join(core, name)).href);
	const { ExtensionRunner } = await load("extensions/runner.ts");
	const { createExtensionRuntime, loadExtensionFromFactory } = await load("extensions/loader.ts");
	const { SessionManager } = await load("session-manager.ts");
	const { ModelRegistry } = await load("model-registry.ts");
	const { AuthStorage } = await load("auth-storage.ts");
	const { createEventBus } = await load("event-bus.ts");
	const { SettingsManager } = await load("settings-manager.ts");
	const { initTheme } = await load("../modes/interactive/theme/theme.ts");
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const runtime = createExtensionRuntime();
		let extensions = request.values ? await Promise.all(request.values.map((value: unknown, index: number) => loadExtensionFromFactory((api: { on: (event: string, handler: () => never) => void }) => api.on("session_start", () => { throw value; }), "/tmp", createEventBus(), runtime, `<inline:${index}>`))) : [];
		if (request.operation === "approval_commands") {
			const module = await load("../../extensions/approval-controls/src/index.ts");
			extensions = [await loadExtensionFromFactory(module.default, "/tmp", createEventBus(), runtime, "<inline>")];
		}
		const settings = ["sandbox", "approval_commands"].includes(request.operation) ? SettingsManager.inMemory() : undefined;
		const runner = new ExtensionRunner(extensions, runtime, "/tmp", SessionManager.inMemory("/tmp"), ModelRegistry.inMemory(AuthStorage.inMemory()), settings);
		let output: unknown;
		if (request.operation === "approval_commands") {
			initTheme("dark", false);
			const { theme } = await load("../modes/interactive/theme/theme.ts");
			let notify: unknown[] = [], status: unknown[] = [];
			runner.setUIContext({ theme, notify: (message: string, type?: string) => notify.push([message, type ?? null]), setStatus: (key: string, text?: string) => status.push([key, text ?? null]) }, "tui");
			const steps: unknown[] = [];
			for (const action of request.actions) {
				let error: string | null = null;
				try { await runner.getCommand(action.name).handler(action.args, runner.createCommandContext()); } catch (failure) { error = failure instanceof Error ? failure.message : String(failure); }
				steps.push({ error, notify, status, profile: settings.getExplicitSandboxProfile() ?? null });
				notify = []; status = [];
			}
			output = steps;
		} else if (request.operation === "sandbox") {
			initTheme("dark", false);
			const results: unknown[] = [];
			for (const action of request.actions) {
				if (action.reset) runner.bindCommandContext();
				try { runner.createCommandContext().setSandboxProfile(action.profile, action.scope); results.push(null); } catch (error) { results.push(error instanceof Error ? error.message : String(error)); }
			}
			output = { results, profile: settings.getExplicitSandboxProfile() ?? null };
		} else if (request.values) {
			const errors: unknown[] = [];
			runner.onError((error: { error: string }) => errors.push(error.error));
			await runner.emit({ type: "session_start", reason: "startup" });
			output = errors;
		} else {
			const before = runner.createContext();
			const commandBefore = runner.createCommandContext();
			let model = { id: "first" };
			runner.getModel = () => model;
			const after = runner.createContext();
			runner.getModel = () => ({ id: "replacement" });
			model = { id: "mutated" };
			output = [before.model?.id ?? null, commandBefore.model?.id ?? null, after.model?.id ?? null, runner.createContext().model?.id ?? null];
		}
		process.stdout.write(`${JSON.stringify({ id: request.id, output })}\n`);
	}
}
void main().catch((error: unknown) => { console.error(error); process.exit(1); });
