import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const units = (value: string): number[] =>
	Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
const gif = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
const png =
	"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aTl0AAAAASUVORK5CYII=";
async function main(): Promise<void> {
	const source = process.argv[2]!;
	const tui = await import(
		pathToFileURL(`${source}/../../tui/src/index.ts`).href
	);
	const { ToolExecutionComponent } = await import(
		pathToFileURL(`${source}/modes/interactive/components/tool-execution.ts`)
			.href
	);
	const themes = await import(
		pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href
	);
	async function run(request: any): Promise<unknown> {
		tui.setCapabilities({
			images: request.kind === "no-caps" ? null : "kitty",
			trueColor: true,
			hyperlinks: false,
		});
		themes.setThemeInstance(
			new themes.Theme(request.colors, request.colors, "truecolor"),
		);
		const events: string[] = [];
		const requests: number[] = [];
		const definition = {
			name: "unknown_tool",
			renderShell: request.shell,
			renderCall() {
				events.push("call");
				return new tui.Text("call", 0, 0);
			},
			renderResult() {
				events.push("result");
				return new tui.Text("result", 0, 0);
			},
		};
		let component: any = new ToolExecutionComponent(
			"unknown_tool",
			"call-id",
			{},
			{ hideOutput: request.hide },
			definition,
			{
				requestRender() {
					requests.push(events.length);
				},
			},
			"/repo",
		);
		const result = (data: string, mimeType: string) => ({
			content: [{ type: "image", data, mimeType }],
			isError: false,
		});
		const snapshot = (label: string) => ({
			label,
			lines: component
				? component
						.render(30)
						.map((line: string) => units(line.replace(/,i=\d+/g, ",i=ID")))
				: [],
			events: [...events],
			requests: [...requests],
		});
		const data =
			request.kind === "png"
				? png
				: request.kind === "invalid"
					? "AAAA"
					: request.kind === "empty"
						? ""
						: gif;
		const mime = request.kind === "png" ? "image/png" : "image/gif";
		component.updateResult(result(data, mime), true);
		const output = [snapshot("first-sync")];
		if (request.replace) component.updateResult(result(png, "image/png"), true);
		else component.updateResult(result(data, mime), true);
		output.push(snapshot("second-sync"));
		if (request.drop) component = undefined;
		const expected =
			request.kind && request.kind !== "gif" ? 0 : request.replace ? 1 : 2;
		if (expected === 0) await new Promise((resolve) => setTimeout(resolve, 20));
		const deadline = Date.now() + 3000;
		while (requests.length < expected && Date.now() < deadline)
			await new Promise((resolve) => setTimeout(resolve, 5));
		if (requests.length !== expected)
			throw new Error(
				`conversion requests ${requests.length}, expected ${expected}`,
			);
		output.push(snapshot("settled"));
		if (component) {
			component.setHideOutput(false);
			output.push(snapshot("unhidden"));
			component.invalidate();
			output.push(snapshot("invalidate"));
			component.updateResult(result(data, mime), true);
			output.push(snapshot("cached"));
		}
		return output;
	}
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			process.stdout.write(
				`${JSON.stringify({ ok: true, output: await run(JSON.parse(line)) })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}
}
void main();
