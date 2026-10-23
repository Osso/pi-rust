import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode, bindInteractiveModeSessionMutationTargetResolver } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const errors: string[] = [];
		const statuses: string[] = [];
		const main = { model: request.models[0], scopedModels: [], async cycleModel() { return undefined; } };
		const child = {
			model: request.models[0],
			scopedModels: request.models.slice(0, request.scope).map((model) => ({ model, thinkingLevel: "off" })),
			modelRegistry: {},
			async cycleModel() {
				if (child.scopedModels.length < 2) return undefined;
				child.model = request.models[1];
				return { model: child.model, thinkingLevel: "off" };
			},
		};
		const receiver = {
			session: main,
			childViewAgentId: request.child ? String.fromCharCode(...request.child) : undefined,
			resolveViewedSessionTarget() { return InteractiveMode.prototype.resolveViewedSessionTarget.call(receiver); },
			showError: (text: string) => errors.push(text),
			showStatus: (text: string) => statuses.push(text),
			footer: { invalidate() {} },
			updateEditorBorderColor() {},
			maybeWarnAboutAnthropicSubscriptionAuth() {},
		};
		if (request.target === "core") bindInteractiveModeSessionMutationTargetResolver(receiver, () => main);
		if (request.target === "interactive") bindInteractiveModeSessionMutationTargetResolver(receiver, () => child);
		await InteractiveMode.prototype.cycleModel.call(receiver, "forward");
		console.log(JSON.stringify({ ok: true, output: { main: main.model.id, child: child.model.id, errors: errors.map(units), statuses } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
