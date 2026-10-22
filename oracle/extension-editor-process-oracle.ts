import { createInterface } from "node:readline";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

type Request = {
	command: string;
	nextCommand?: string;
	report: string;
	explicit: boolean;
	prefill: number[];
};

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { ExtensionEditorComponent } = await import(
		pathToFileURL(`${source}/modes/interactive/components/extension-editor.ts`)
			.href
	);
	const { KeybindingsManager } = await import(
		pathToFileURL(`${source}/core/keybindings.ts`).href
	);
	const themes = await import(
		pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href
	);
	themes.setThemeInstance(
		new themes.Theme(
			{ accent: 0, text: 0, muted: 0, dim: 0, border: 0, borderMuted: 0 },
			{},
			"256color",
		),
	);
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request: Request = JSON.parse(line);
			const output = await observe(request);
			process.stdout.write(`${JSON.stringify({ ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}

	async function observe(request: Request): Promise<unknown> {
		const events: string[] = [];
		const submissions: number[][] = [];
		let finish!: () => void;
		const complete = new Promise<void>((resolve) => {
			finish = resolve;
		});
		const host = {
			terminal: { rows: 24 },
			requestRender(force?: boolean) {
				if (force) {
					events.push("render");
					finish();
				}
			},
			createFixedCell() {
				return {
					place() {},
					clear() {},
					update() {
						return false;
					},
				};
			},
			stop() {
				events.push("stop");
			},
			start() {
				events.push("start");
			},
		};
		const units = (text: string): number[] =>
			Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
		process.env.VISUAL = request.command;
		const editor = new ExtensionEditorComponent(
			host,
			new KeybindingsManager(),
			"Compose",
			String.fromCharCode(...request.prefill),
			(value: string) => submissions.push(units(value)),
			() => events.push("cancel"),
			undefined,
			request.explicit ? request.command : undefined,
		);
		editor.focused = true;
		if (request.nextCommand !== undefined)
			process.env.VISUAL = request.nextCommand;
		const write = process.stdout.write;
		process.stdout.write = function (chunk: string | Uint8Array): boolean {
			if (String(chunk).startsWith("Launching external editor:")) return true;
			return write.call(process.stdout, chunk);
		};
		try {
			editor.handleInput("\x07");
			events.push("input-returned");
			await Promise.race([
				complete,
				new Promise((_, reject) =>
					setTimeout(() => reject(new Error("external editor timeout")), 3000),
				),
			]);
		} finally {
			process.stdout.write = write;
		}
		editor.handleInput("\r");
		const report = JSON.parse(readFileSync(request.report, "utf8"));
		delete report.path;
		return { events, submissions, report, focused: editor.focused };
	}
}
void main();
