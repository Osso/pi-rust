import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const themes = await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
type ModelWire = { id: string; provider: string };
type Renderable = { render(width: number): string[]; handleInput(data: string): void; focused?: boolean };
const units = (text: string) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));

for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as { models: ModelWire[]; search: string | null; keys: string[]; colors: string[] };
		if (request.models.length === 0) throw new Error("fixture models required");
		themes.setThemeInstance(new themes.Theme(Object.fromEntries(request.colors.map((name) => [name, 0])), {}, "256color"));
		let selector: Renderable | undefined;
		const statuses: string[] = [];
		const target = {
			model: request.models[0],
			scopedModels: [],
			modelRegistry: {
				refresh: () => {},
				getError: () => undefined,
				getAvailable: () => request.models,
				find: (provider: string, id: string) => request.models.find((model) => model.provider === provider && model.id === id),
			},
			setModel: async (model: ModelWire) => { target.model = model; },
		};
		const receiver = {
			ui: { requestRender: () => {} },
			footer: { invalidate: () => {} },
			resolveViewedSessionTarget: () => target,
			getModelCandidates: () => InteractiveMode.prototype.getModelCandidates.call(receiver),
			findExactModelMatch: (term: string) => InteractiveMode.prototype.findExactModelMatch.call(receiver, term),
			showModelSelector: (term?: string) => InteractiveMode.prototype.showModelSelector.call(receiver, term),
			showSelector: (factory: (done: () => void) => { component: Renderable; focus?: Renderable }) => {
				const mounted = factory(() => { selector = undefined; });
				selector = mounted.component;
				(mounted.focus ?? mounted.component).focused = true;
			},
			updateEditorBorderColor: () => {},
			showStatus: (message: string) => { statuses.push(message); },
			showError: (message: string) => { statuses.push(message); },
			maybeWarnAboutAnthropicSubscriptionAuth: async () => {},
			checkDaxnutsEasterEgg: () => {},
		};
		await InteractiveMode.prototype.handleModelCommand.call(receiver, request.search ?? undefined);
		await new Promise((resolve) => setImmediate(resolve));
		for (const key of request.keys) {
			if (!selector) throw new Error("selector not mounted for input");
			selector.handleInput(key);
			await new Promise((resolve) => setImmediate(resolve));
		}
		console.log(JSON.stringify({ ok: true, output: {
			model: target.model.id,
			selectorLines: selector ? selector.render(100).map(units) : null,
			statuses,
		} }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
