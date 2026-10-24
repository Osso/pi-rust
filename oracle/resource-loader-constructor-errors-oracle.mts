import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { DefaultResourceLoader } = await import(
	pathToFileURL(`${root}/core/resource-loader.ts`).href
);
const { SettingsManager } = await import(
	pathToFileURL(`${root}/core/settings-manager.ts`).href
);
const { AuthStorage } = await import(
	pathToFileURL(`${root}/core/auth-storage.ts`).href
);
const { ModelRegistry } = await import(
	pathToFileURL(`${root}/core/model-registry.ts`).href
);
const { SessionManager } = await import(
	pathToFileURL(`${root}/core/session-manager.ts`).href
);
const { createAgentSession } = await import(
	pathToFileURL(`${root}/core/sdk.ts`).href
);
const { createAgentSessionServices } = await import(
	pathToFileURL(`${root}/core/agent-session-services.ts`).href
);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			operation: string;
			cwd: string;
			agentDir: string;
			marker: string;
			models: string;
			allow: number;
		};
		const settingsManager = SettingsManager.inMemory();
		const authStorage = AuthStorage.inMemory();
		const modelRegistry = new ModelRegistry(authStorage, request.models);
		const sessionManager = SessionManager.inMemory(request.cwd);
		process.env.PI_TEST_NSS_FAILURE_FILE = request.marker;
		process.env.PI_TEST_NSS_ALLOW = String(request.allow);
		delete process.env.HOME;
		writeFileSync(request.marker, "");
		let error: unknown;
		try {
			if (request.operation === "sdk")
				await createAgentSession({
					cwd: request.cwd,
					agentDir: request.agentDir,
					settingsManager,
					authStorage,
					modelRegistry,
					sessionManager,
					noSupervisor: true,
				});
			else if (request.operation === "services")
				await createAgentSessionServices({
					cwd: request.cwd,
					agentDir: request.agentDir,
					settingsManager,
					authStorage,
					modelRegistry,
				});
			else
				new DefaultResourceLoader({
					cwd: request.cwd,
					agentDir: request.agentDir,
					settingsManager,
				});
		} catch (value) {
			if (value instanceof Error) {
				const properties: Record<string, unknown> = {
					name: value.name,
					message: value.message,
				};
				for (const key of Object.getOwnPropertyNames(value))
					if (key !== "stack") properties[key] = Reflect.get(value, key);
				error = properties;
			} else error = value;
		}
		console.log(JSON.stringify({ ok: true, output: error ?? null }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
