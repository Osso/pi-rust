// Execute complete pinned client. Real Input/SelectList/Container; process,
// terminal scheduling and RPC event delivery are fixture boundaries.
import { PassThrough } from "node:stream";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import vm from "node:vm";
import { EventEmitter } from "node:events";
import { createInterface } from "node:readline";
const file = process.argv[2],
	root = resolve(dirname(file), "../../.."),
	require = createRequire(resolve(root, "package.json")),
	ts = require("typescript");
const source = readFileSync(file, "utf8").replaceAll(
	"import.meta.url",
	JSON.stringify(pathToFileURL(file).href),
);
const js = ts.transpileModule(source, {
	compilerOptions: {
		target: ts.ScriptTarget.ES2022,
		module: ts.ModuleKind.CommonJS,
	},
}).outputText;
async function main() {
	const tui = await import(
		pathToFileURL(resolve(root, "packages/tui/src/index.ts")).href
	);
	for await (const line of createInterface({ input: process.stdin })) {
		const req = JSON.parse(line);
		let active: any,
			screen: any,
			exited = false;
		const commands: any[] = [],
			timers = new Map<number, () => void>();
		let timer = 0,
			screenStarted = false,
			started: () => void = () => {},
			rejectStartup: (error: Error) => void = () => {};
		const ready = new Promise<void>((resolve, reject) => {
			started = () => {
				screenStarted = true;
				resolve();
			};
			rejectStartup = reject;
		});
		class Screen {
			children: any[] = [];
			constructor() {
				screen = this;
			}
			addChild(c: any) {
				this.children.push(c);
			}
			setFocus(c: any) {
				if (active && "focused" in active) active.focused = false;
				active = c;
				if (active && "focused" in active) active.focused = true;
			}
			requestRender() {}
			start() {
				started();
			}
			stop() {}
			render(w: number) {
				return this.children.flatMap((c) => c.render(w));
			}
		}
		const stdout = new PassThrough();
		const reader = createInterface({ input: stdout });
		const agent: any = new EventEmitter();
		agent.stderr = new EventEmitter();
		agent.stdout = stdout;
		agent.exitCode = null;
		agent.stdin = { write: (s: string) => commands.push(JSON.parse(s)) };
		agent.kill = () => {};
		const context: any = {
			exports: {},
			console: {
				error: (...args: unknown[]) => console.error(...args),
				log: () => {},
			},
			process: {
				exit: (code = 0) => {
					exited = true;
					if (!screenStarted)
						rejectStartup(
							new Error(
								`RPC fixture exited before screen start (code ${code})`,
							),
						);
				},
			},
			setTimeout: (cb: () => void) => {
				queueMicrotask(cb);
				return 1;
			},
			setInterval: (cb: () => void) => {
				timers.set(++timer, cb);
				return timer;
			},
			clearInterval: (id: number) => timers.delete(id),
			require: (name: string) => {
				if (name === "node:child_process") return { spawn: () => agent };
				if (name === "node:readline") return { createInterface: () => reader };
				if (name === "@earendil-works/pi-tui")
					return {
						...tui,
						TuiMainScreen: Screen,
						ProcessTerminal: class {
							constructor() {
								if (req.terminalStartupError)
									throw new Error(req.terminalStartupError);
							}
						},
					};
				return require(name);
			},
		};
		vm.runInNewContext(js, context, { filename: file });
		await ready;
		const steps = [];
		for (const op of req.ops) {
			if (op.raw) {
				stdout.write(Buffer.from(op.raw));
				await new Promise<void>((r) => setImmediate(r));
			}
			if (op.event) reader.emit("line", JSON.stringify(op.event));
			if (op.input) active.handleInput(op.input);
			if (op.tick) for (const cb of timers.values()) cb();
			steps.push({
				renders: req.widths.map((w: number) => screen.render(w)),
				commands: commands.splice(0),
				exit: exited,
			});
		}
		reader.close();
		stdout.destroy();
		console.log(JSON.stringify({ steps }));
	}
}
main().catch((e) => {
	console.error(e);
	process.exit(1);
});
