import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Step = { op: string; units?: number[]; value?: boolean };
type Request = {
	models: { id: string; name: string; provider: string }[];
	enabled: number[][] | null;
	width: number;
	steps: Step[];
	bindings: Record<string, string[]>;
	children?: boolean;
	mutateCallbacks?: boolean;
	callbackError?: string;
};
const text = (value: number[]): string => String.fromCharCode(...value);
const units = (value: string): number[] =>
	Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const { setKeybindings } = await import(
		pathToFileURL(`${source}/../../tui/src/index.ts`).href
	);
	const { ScopedModelsSelectorComponent } = await import(
		pathToFileURL(
			`${source}/modes/interactive/components/scoped-models-selector.ts`,
		).href
	);
	const { KeybindingsManager } = await import(
		pathToFileURL(`${source}/core/keybindings.ts`).href
	);
	const themes = await import(
		pathToFileURL(`${source}/modes/interactive/theme/theme.ts`).href
	);
	themes.setThemeInstance(
		new themes.Theme(
			{ accent: 0, muted: 0, dim: 0, success: 0, warning: 0, border: 0 },
			{},
			"256color",
		),
	);
	function run(request: Request, uncaught = false): unknown {
		setKeybindings(new KeybindingsManager(request.bindings));
		const events: unknown[] = [];
		const callback = (kind: string, ids: string[] | null): void => {
			if (uncaught) process.stdout.write(`${kind}\n`);
			events.push({ kind, ids: ids?.map(units) ?? null });
			if (request.mutateCallbacks) ids?.push("callback/mutation");
			if (request.callbackError === kind) throw new Error(`callback ${kind}`);
		};
		const component = new ScopedModelsSelectorComponent(
			{
				allModels: request.models,
				enabledModelIds: request.enabled?.map(text) ?? null,
			},
			{
				onChange: (ids: string[] | null) => callback("change", ids),
				onPersist: (ids: string[] | null) => callback("persist", ids),
				onCancel: () => {
					events.push({ kind: "cancel" });
					if (request.callbackError === "cancel")
						throw new Error("callback cancel");
				},
			},
		);
		let extraInvalidations = 0;
		const extra = {
			render: () => [
				`extra${extraInvalidations}${String.fromCharCode(0xd800)}`,
			],
			invalidate: () => {
				extraInvalidations++;
			},
		};
		const snapshot = (error?: string): unknown => ({
			lines: component.render(request.width).map(units),
			search: units(component.getSearchInput().getValue()),
			focused: component.getSearchInput().focused,
			events: [...events],
			...(request.children
				? {
						children: component.children
							.flatMap((child: { render(width: number): string[] }) =>
								child.render(request.width),
							)
							.map(units),
					}
				: {}),
			...(error ? { error } : {}),
		});
		const output = [snapshot()];
		for (const step of request.steps) {
			let error: string | undefined;
			try {
				switch (step.op) {
					case "key":
						component.handleInput(text(step.units!));
						break;
					case "search":
						component.getSearchInput().setValue(text(step.units!));
						break;
					case "focus":
						component.focused = step.value;
						break;
					case "add":
						component.addChild(extra);
						break;
					case "remove":
						component.removeChild(extra);
						break;
					case "clear":
						component.clear();
						break;
					case "invalidate":
						component.invalidate();
						break;
					default:
						throw new Error(`unknown operation ${step.op}`);
				}
			} catch (caught) {
				if (uncaught) throw caught;
				error = String(caught);
			}
			output.push(snapshot(error));
		}
		return output;
	}
	if (process.env.PI_SCOPED_UNCAUGHT === "1") {
		process.stdout.write("before\n");
		run(
			{
				models: [
					{ id: "a", name: "Alpha", provider: "provider" },
					{ id: "b", name: "Beta", provider: "provider" },
				],
				enabled: [units("provider/a"), units("provider/b")],
				width: 80,
				bindings: {},
				steps: [
					{ op: "search", units: units("Beta") },
					{ op: "key", units: units("\u001b[C") },
					{ op: "key", units: units("\u001b[1;3A") },
				],
			},
			true,
		);
		process.stdout.write("after\n");
		return;
	}
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			process.stdout.write(
				`${JSON.stringify({ ok: true, output: run(JSON.parse(line)) })}\n`,
			);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ ok: false, error: String(error) })}\n`,
			);
		}
	}
}
void main();
