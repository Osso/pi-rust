import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Request = { stats: unknown; name: number[] | null; width: number };
async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { InteractiveMode } = await import(
		pathToFileURL(`${source}/modes/interactive/interactive-mode.ts`).href
	);
	const { Container } = await import(
		pathToFileURL(`${source}/../../tui/src/index.ts`).href
	);
	const themes = await import(
		pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href
	);
	themes.setThemeInstance(new themes.Theme({ dim: 0 }, {}, "256color"));
	const units = (text: string): number[] =>
		Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line) as Request;
			const chatContainer = new Container();
			const receiver = {
				session: { getSessionStats: () => request.stats },
				sessionManager: {
					getSessionName: () =>
						request.name === null
							? undefined
							: String.fromCharCode(...request.name),
				},
				chatContainer,
				ui: { requestRender: () => {} },
			};
			InteractiveMode.prototype.handleSessionCommand.call(receiver);
			process.stdout.write(
				`${JSON.stringify({ ok: true, output: { lines: chatContainer.render(request.width).map(units) } })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}
}
void main();
