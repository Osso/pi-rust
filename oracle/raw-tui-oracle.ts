import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { spawnSync } from "node:child_process";

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const utils = await import(pathToFileURL(path.join(source, "utils.ts")).href);
	const { Input } = await import(pathToFileURL(path.join(source, "components/input.ts")).href);
	const { Text } = await import(pathToFileURL(path.join(source, "components/text.ts")).href);
	const native = (units: number[]): string => String.fromCharCode(...units);
	
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			let output: unknown;
			if (request.operation === "utils") {
				const text = native(request.units);
				output = {
					width: utils.visibleWidth(text),
					wrap: utils.wrapTextWithAnsi(text, request.width),
					truncate: utils.truncateToWidth(text, request.width),
					slice: utils.sliceByColumn(text, request.start, request.width, true),
					segments: utils.extractSegments(text, request.start, request.start + 1, request.width, true),
					normalize: utils.normalizeTerminalOutput(text),
				};
			} else if (request.operation === "text") {
				const text = new Text(native(request.units), request.paddingX ?? 0, request.paddingY ?? 0);
				if (request.background) text.setCustomBgFn((value: string) => `\x1b[44m${value}\x1b[0m`);
				output = [text.render(request.width), text.render(request.width)];
			} else if (request.operation === "input") {
				const input = new Input({ mask: request.mask ?? false });
				const submitted: string[] = [];
				input.onSubmit = (value: string) => submitted.push(value);
				const states: unknown[] = [];
				for (const action of request.actions) {
					if (action.set) input.setValue(native(action.set));
					if (action.key) input.handleInput(native(action.key));
					states.push({ value: input.getValue(), cursor: (input as unknown as { cursor: number }).cursor, lines: input.render(request.width) });
				}
				output = { states, submitted };
			} else if (request.operation === "frame") {
                output = await captureFrame(source, native(request.units));
            } else if (request.operation === "terminal_bytes") {
                const child = spawnSync(process.execPath, ["-e", "process.stdout.write(JSON.parse(process.argv[1]))", JSON.stringify(native(request.units))]);
                if (child.status !== 0) throw new Error(child.stderr.toString());
                output = child.stdout.toString("hex");
            } else {
				throw new Error(`Unknown operation: ${request.operation}`);
			}
			// Preserve the source's native string serialization; no scalar conversion.
			process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(output) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
		}
	}
}
void main();

class RawTerminal {
    columns = 12;
    rows = 5;
    kittyProtocolActive = true;
    writes: string[] = [];
    start(): void { this.write("\x1b[?2004h"); }
    stop(): void { this.write("\x1b[?2004l"); }
    async drainInput(): Promise<void> {}
    write(value: string): void { this.writes.push(value); }
    moveBy(lines: number): void {
        if (lines > 0) this.write(`\x1b[${lines}B`);
        else if (lines < 0) this.write(`\x1b[${-lines}A`);
    }
    hideCursor(): void { this.write("\x1b[?25l"); }
    showCursor(): void { this.write("\x1b[?25h"); }
    clearLine(): void { this.write("\x1b[K"); }
    clearFromCursor(): void { this.write("\x1b[J"); }
    clearScreen(): void { this.write("\x1b[2J\x1b[H"); }
    setTitle(): void {}
    setProgress(): void {}
    takeWrites(): string[] { return this.writes.splice(0); }
}

async function captureFrame(source: string, raw: string): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "tui.ts")).href);
    const images = await import(pathToFileURL(path.join(source, "terminal-image.ts")).href);
    images.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
    const terminal = new RawTerminal();
    const tui = new module.TUI(terminal);
    const lines = [`x${raw}`, "tail"];
    const frames: string[][] = [];
    tui.addChild({ render: () => lines, invalidate() {} });
    const flush = (): void => {
        tui.renderPendingState();
        frames.push(terminal.takeWrites());
    };
    tui.start();
    flush();
    lines[0] = `y${raw}`;
    tui.requestRender();
    flush();
    const handle = tui.showOverlay({ render: () => [raw], invalidate() {} }, { width: 4, row: 0, col: 2 });
    flush();
    handle.hide();
    flush();
    const cell = tui.createFixedCell();
    cell.place(0, 1);
    const updated = cell.update("\ud800z\udc00");
    frames.push(terminal.takeWrites());
    tui.stop();
    frames.push(terminal.takeWrites());
    const stop = frames.pop()!.join("");
    return [frames, stop, updated];
}
