import childProcess from "node:child_process";
import { syncBuiltinESMExports } from "node:module";
import { dirname } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
let operation = "";
let observed: string | undefined;
const savedSpawn = childProcess.spawnSync;
childProcess.spawnSync = ((program: string, args: string[]) => {
	if (program === "gh")
		return { status: 0, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) };
	if (program === "wslpath") {
		observed = args[1];
		return { status: 0, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) };
	}
	if (operation === "image" && program === "wl-paste")
		return {
			status: 0,
			stdout: Buffer.from(
				args[0] === "--list-types" ? "image/png\n" : "clipboard-image-bytes",
			),
			stderr: Buffer.alloc(0),
		};
	return { status: 1, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) };
}) as typeof childProcess.spawnSync;
syncBuiltinESMExports();
const { InteractiveMode } = await import(
	pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href
);
const { readClipboardImage } = await import(
	pathToFileURL(`${root}/utils/clipboard-image.ts`).href
);
const fs = await import("node:fs");
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as {
			operation: string;
			env: Record<string, string>;
		};
		operation = request.operation;
		observed = undefined;
		for (const key of ["TMPDIR", "TMP", "TEMP"]) delete process.env[key];
		Object.assign(process.env, request.env);
		if (operation === "powershell")
			await readClipboardImage({
				platform: "linux",
				env: { WAYLAND_DISPLAY: "fixture", WSL_DISTRO_NAME: "fixture" },
			});
		else if (operation === "image" || operation === "image-text") {
			process.env.WAYLAND_DISPLAY = "fixture";
			const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
				editor: {
					insertTextAtCursor: (path: string) => {
						observed = path;
					},
				},
				clipboardTempFiles: { track() {} },
				ui: { requestRender() {} },
			});
			await InteractiveMode.prototype.handleClipboardImagePaste.call(receiver);
		} else {
			const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
				runtimeHost: {
					session: {
						exportToHtml: async (path: string) => {
							observed = path;
							throw new Error("path probe completed");
						},
					},
				},
				showError() {},
			});
			await InteractiveMode.prototype.handleShareCommand.call(receiver);
		}
		if (operation === "image-text") {
			console.log(
				JSON.stringify({
					ok: true,
					output: { unchanged: observed === undefined },
				}),
			);
			continue;
		}
		if (observed === undefined)
			throw new Error("actual source did not reach temp path consumer");
		const root = dirname(observed);
		if (operation === "image") fs.unlinkSync(observed);
		console.log(JSON.stringify({ ok: true, output: { root } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
childProcess.spawnSync = savedSpawn;
syncBuiltinESMExports();
