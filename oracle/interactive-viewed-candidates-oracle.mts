import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode, bindInteractiveModeSessionMutationTargetResolver } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const target = {
			scopedModels: request.scoped ? [{ model: request.models[1] }] : [],
			modelRegistry: { refresh() {}, getAvailable: () => request.models },
			cycleModel() {},
		};
		const receiver = {
			session: target,
			childViewAgentId: request.view ? "child" : undefined,
			resolveViewedSessionTarget() { return InteractiveMode.prototype.resolveViewedSessionTarget.call(receiver); },
		};
		if (request.target !== "missing") {
			bindInteractiveModeSessionMutationTargetResolver(receiver, () => {
				if (request.target === "throw") {
					const error = new TypeError(String.fromCharCode(...request.message));
					Object.assign(error, { code: "E_TARGET" });
					throw error;
				}
				return request.target === "core" ? {} : target;
			});
		}
		try {
			const models = await InteractiveMode.prototype.getModelCandidates.call(receiver);
			console.log(JSON.stringify({ ok: true, output: { models: models.map((model) => model.id) } }));
		} catch (error) {
			console.log(JSON.stringify({ ok: true, output: { error: { name: error.name, message: units(error.message), code: error.code ?? null } } }));
		}
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
