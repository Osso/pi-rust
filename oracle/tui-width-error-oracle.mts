import fs from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("tui source root required");
const { TUI } = await import(pathToFileURL(path.join(source, "tui.ts")).href);
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		process.env.HOME = request.home;
		const events: unknown[] = [];
		let rows = ["safe"];
		const stopError = "stop-\ud800";
		const terminal = {
			columns: 12,
			rows: 24,
			kittyProtocolActive: false,
			start() {},
			stop() {
				events.push({
					stop: true,
					log: fs.existsSync(path.join(request.home, ".pi/agent/pi-crash.log")),
				});
				if (request.stopError) throw stopError;
			},
			write() {},
			hideCursor() {},
			showCursor() {},
			drainInput() {},
			moveBy() {},
			clearLine() {},
			clearFromCursor() {},
			clearScreen() {},
			setTitle() {},
			setProgress() {},
		};
		const tui = new TUI(terminal);
		tui.addChild({ render: () => rows, invalidate() {} });
		tui.start();
		tui.doRender();
		rows = ["overwidth-overwidth"];
		let error: unknown;
		try {
			tui.doRender();
		} catch (value) {
			error =
				value instanceof Error
					? { name: value.name, message: value.message }
					: typeof value === "string"
						? {
								rawUnits: Array.from({ length: value.length }, (_, index) =>
									value.charCodeAt(index),
								),
							}
						: value;
		}
		const log = fs.readFileSync(
			path.join(request.home, ".pi/agent/pi-crash.log"),
			"utf8",
		);
		console.log(
			JSON.stringify({
				ok: true,
				output: { events, error, log: log.split("\n").slice(1).join("\n") },
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
