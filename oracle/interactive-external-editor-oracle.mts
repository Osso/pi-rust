import {
	appendFileSync,
	existsSync,
	readdirSync,
	readFileSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(
	pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href
);
const { TUI, Container } = await import(
	pathToFileURL(`${root}/../../tui/src/index.ts`).href
);
interface Request {
	directory: string;
	trace: string;
	heartbeat: string;
	command: string;
	text: string;
	expandedText?: string;
	now: number;
}
const units = (text: string) =>
	Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const errorWire = (error: unknown) => {
	if (!(error instanceof Error)) return error;
	const output: Record<string, unknown> = {
		name: error.name,
		message: error.message,
	};
	for (const key of ["code", "errno", "syscall", "path"])
		if (Object.hasOwn(error, key)) output[key] = Reflect.get(error, key);
	return output;
};
for await (const line of createInterface({ input: process.stdin })) {
	const savedNow = Date.now;
	const savedWrite = process.stdout.write;
	const savedTmp = process.env.TMPDIR;
	let ui: InstanceType<typeof TUI> | undefined;
	let recording = false;
	try {
		const request = JSON.parse(line) as Request;
		writeFileSync(request.trace, "");
		const snapshot = (event: string) => {
			if (!recording) return;
			const files = readdirSync(request.directory)
				.sort()
				.map((name) => {
					const path = join(request.directory, name);
					return {
						name,
						bytes: statSync(path).isFile()
							? Array.from(readFileSync(path))
							: null,
					};
				});
			appendFileSync(request.trace, `${JSON.stringify({ event, files })}\n`);
		};
		const terminal = {
			columns: 100,
			rows: 24,
			kittyProtocolActive: false,
			start() {
				snapshot("start");
			},
			stop() {
				snapshot("stop");
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
		ui = new TUI(terminal);
		ui.start();
		recording = true;
		process.env.TMPDIR = request.directory;
		Date.now = () => request.now;
		let text = request.text;
		const banners: string[] = [];
		process.stdout.write = ((chunk: unknown) => {
			banners.push(String(chunk));
			return true;
		}) as typeof process.stdout.write;
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			runtimeHost: {
				session: {
					settingsManager: { getExternalEditorCommand: () => request.command },
				},
			},
			editor: {
				getText: () => text,
				getExpandedText: () => request.expandedText ?? text,
				setText: (value: string) => {
					text = value;
				},
			},
			chatContainer: new Container(),
			ui,
		});
		setImmediate(() => writeFileSync(request.heartbeat, "ready"));
		let error: unknown = null;
		try {
			await InteractiveMode.prototype.openExternalEditor.call(receiver);
		} catch (thrown) {
			error = errorWire(thrown);
		}
		const trace = readFileSync(request.trace, "utf8")
			.split("\n")
			.filter(Boolean)
			.map((entry) => JSON.parse(entry));
		process.stdout.write = savedWrite;
		recording = false;
		ui.stop();
		ui = undefined;
		console.log(
			JSON.stringify({
				ok: true,
				output: {
					trace,
					text: units(text),
					chat: receiver.chatContainer.render(100).map(units),
					banners,
					error,
					heartbeat: existsSync(request.heartbeat),
				},
			}),
		);
	} catch (error) {
		process.stdout.write = savedWrite;
		recording = false;
		ui?.stop();
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	} finally {
		Date.now = savedNow;
		process.stdout.write = savedWrite;
		if (savedTmp === undefined) delete process.env.TMPDIR;
		else process.env.TMPDIR = savedTmp;
	}
}
