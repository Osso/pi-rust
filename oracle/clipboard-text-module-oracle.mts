import fs from "node:fs";
import Module, { createRequire, syncBuiltinESMExports } from "node:module";
import os from "node:os";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
const state = process.env.PI_CLIPBOARD_TEXT_STATE;
if (!root || !state) throw new Error("source root and private state required");
const config = JSON.parse(fs.readFileSync(`${state}/config.json`, "utf8"));
process.on("uncaughtException", (error) => {
	fs.writeFileSync(`${state}/uncaught.json`, JSON.stringify(Object.assign({ name: error.name, message: error.message }, error)));
	process.exit(1);
});
const requireSource = createRequire(`${root}/utils/clipboard-native.ts`);
const nativePath = requireSource.resolve("@mariozechner/clipboard");
const nativeModule = new Module(nativePath);
nativeModule.loaded = true;
nativeModule.exports = config.nativeAvailable
	? {
			setText: async (text: string) => {
				fs.writeFileSync(`${state}/native-input`, Buffer.from(text));
				if (config.nativeError) throw new Error("native failed");
			},
		}
	: null;
requireSource.cache[nativePath] = nativeModule;
// Replace only the OS boundary used by copyToClipboard; execute the frozen module.
os.platform = () => config.platform;
syncBuiltinESMExports();
const { copyToClipboard } = await import(
	pathToFileURL(`${root}/utils/clipboard.ts`).href
);
if (config.closeStdout) {
	fs.writeFileSync(`${state}/copy-ready`, "ready");
	while (!fs.existsSync(`${state}/copy-release`))
		await new Promise((resolve) => setTimeout(resolve, 5));
}
try {
	await copyToClipboard(config.text);
	fs.writeFileSync(`${state}/result.json`, JSON.stringify({ copied: true }));
} catch (error) {
	if (!(error instanceof Error)) throw error;
	fs.writeFileSync(
		`${state}/result.json`,
		JSON.stringify({ error: { name: error.name, message: error.message } }),
	);
}
