import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { DefaultPackageManager } = await import(pathToFileURL(`${root}/core/package-manager.ts`).href);
const { SettingsManager } = await import(pathToFileURL(`${root}/core/settings-manager.ts`).href);
for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { operation: string; base: string; home: string; marker: string };
	delete process.env.HOME;
	process.env.PI_OFFLINE = "0";
	process.env.PI_TEST_PASSWD_HOME = request.home;
	process.env.PI_TEST_PASSWD_FAILURE_FILE = request.marker;
	process.env.PI_TEST_FAIL_PASSWD = request.operation === "constructor" ? "1" : "0";
	try {
		const manager = new DefaultPackageManager({
			cwd: request.base, agentDir: request.base, settingsManager: SettingsManager.inMemory({ packages: ["~"] }),
		});
		if (request.operation !== "constructor") {
			writeFileSync(request.marker, "fail");
			if (request.operation === "resolve") await manager.resolve();
			else if (request.operation === "install") await manager.install("~", false);
			else if (request.operation === "installed-path") manager.getInstalledPath("~", "user");
			else if (request.operation === "identity") manager.getPackageIdentity("~");
			else if (request.operation === "list") manager.listConfiguredPackages();
			else if (request.operation === "remove") manager.removeSourceFromSettings("~");
			else if (request.operation === "add-match") manager.addSourceToSettings("npm:test-package");
			else if (request.operation === "check-updates") await manager.checkForAvailableUpdates();
			else throw new Error(`Unexpected operation: ${request.operation}`);
		}
		console.log(JSON.stringify({ threw: false }));
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		console.log(JSON.stringify({ threw: true, message: error.message, error: String(error) }));
	}
}
