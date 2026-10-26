import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { isDeepStrictEqual } from "node:util";

const lock = JSON.parse(readFileSync(resolve(import.meta.dirname, "../docs/porting/delta-lock.json"), "utf8"));
const delta = lock.deltas.find((entry: { commit: string }) => entry.commit === "5a0ea4d393e21d190524f26f1375486d7b86f7cb");
if (!delta) throw new Error("Terra delta missing from lock");

export async function applyTerraDelta(models: Record<string, Record<string, unknown>>): Promise<void> {
	for (const [provider, symbol] of [["openai-codex", "OPENAI_CODEX_MODELS"], ["openai-codex-gc", "OPENAI_CODEX_GC_MODELS"], ["openai-codex-team", "OPENAI_CODEX_TEAM_MODELS"]]) {
		const relative = `packages/ai/src/providers/${provider}.models.ts`;
		const file = resolve(delta.checkout, relative);
		const hash = createHash("sha256").update(readFileSync(file)).digest("hex");
		if (hash !== delta.modules[relative]) throw new Error(`Terra lock mismatch: ${relative}`);
		const locked = (await import(pathToFileURL(file).href))[symbol];
		const { ["gpt-6.1-terra"]: terra, ...baseline } = locked;
		if (!terra || !isDeepStrictEqual(models[provider], baseline)) throw new Error(`Unexpected Terra delta: ${provider}`);
		models[provider] = locked;
	}
}
