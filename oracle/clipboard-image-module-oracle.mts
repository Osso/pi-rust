import fs from "node:fs";
import { spawnSync } from "node:child_process";
import Module, { createRequire } from "node:module";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const state = process.env.PI_CLIPBOARD_MATRIX_STATE;
if (!state) throw new Error("private state required");
const config = JSON.parse(fs.readFileSync(`${state}/config.json`, "utf8"));
const record = (program: string) =>
	fs.appendFileSync(
		`${state}/trace.jsonl`,
		`${JSON.stringify({ program, args: [] })}\n`,
	);
const sourceRequire = createRequire(`${root}/utils/clipboard-native.ts`);
const nativePath = sourceRequire.resolve("@mariozechner/clipboard");
const nativeModule = new Module(nativePath);
nativeModule.loaded = true;
nativeModule.exports =
	config.nativeAvailable === false
		? null
		: {
				hasImage: () => {
					record("native.hasImage");
					if (config.nativeHasError) throw new Error(config.nativeHasError);
					return config.nativeHas ?? false;
				},
				getImageBinary: async () => {
					record("native.getImageBinary");
					if (config.nativeGetError) throw new Error(config.nativeGetError);
					return [...Buffer.from(config.nativeHex ?? "", "hex")];
				},
			};
sourceRequire.cache[nativePath] = nativeModule;
process.env.DISPLAY = "fixture-no-display-access";
delete process.env.TERMUX_VERSION;
const { readClipboardImage, extensionForImageMimeType, isWaylandSession } =
	await import(pathToFileURL(`${root}/utils/clipboard-image.ts`).href);

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	process.env.PATH = request.bin;
	process.env.TMPDIR = request.temporary;
	try {
		if (request.operation === "sync") {
			const result = spawnSync(
				request.program,
				request.arguments,
				request.options,
			);
			console.log(
				JSON.stringify({
					pidPresent: result.pid > 0,
					status: result.status,
					signal: result.signal,
					stdout: result.stdout ? [...result.stdout] : null,
					stderr: result.stderr ? [...result.stderr] : null,
					error: result.error
						? {
								name: result.error.name,
								message: result.error.message,
								...result.error,
							}
						: null,
				}),
			);
			continue;
		}
		if (request.operation === "helpers") {
			console.log(
				JSON.stringify({
					extensions: request.mimeTypes.map((mime: string) =>
						extensionForImageMimeType(mime),
					),
					wayland: request.environments.map((env: NodeJS.ProcessEnv) =>
						isWaylandSession(env),
					),
				}),
			);
			continue;
		}
		const image = await readClipboardImage({
			env: request.env,
			platform: request.platform,
		});
		console.log(
			JSON.stringify({
				image: image
					? { bytes: [...image.bytes], mimeType: image.mimeType }
					: null,
			}),
		);
	} catch (error) {
		const value =
			error instanceof Error
				? { name: error.name, message: error.message, ...error }
				: { value: error };
		console.log(JSON.stringify({ error: value }));
	}
}
