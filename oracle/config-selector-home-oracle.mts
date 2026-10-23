import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { ConfigSelectorComponent } = await import(
	pathToFileURL(`${root}/modes/interactive/components/config-selector.ts`).href
);
const { SettingsManager } = await import(pathToFileURL(`${root}/core/settings-manager.ts`).href);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { scope: string; source: string };
	const resolved = {
		extensions: [{ path: "/repo/extension.ts", enabled: true, metadata: {
			source: request.source, scope: request.scope, origin: "top-level", baseDir: "/repo",
		} }],
		skills: [], prompts: [], themes: [],
	};
	delete process.env.HOME;
	process.env.PI_TEST_FAIL_PASSWD = "1";
	try {
		new ConfigSelectorComponent(resolved, SettingsManager.inMemory(), "/repo", "/agent", () => {}, () => {}, () => {}, 24);
		console.log(JSON.stringify({ threw: false }));
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		console.log(JSON.stringify({ threw: true, message: error.message, error: String(error) }));
	}
}
