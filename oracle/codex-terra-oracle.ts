import { createInterface } from "node:readline";
import { OPENAI_CODEX_MODELS } from "/home/osso/.worktrees/pi-delta-terra-5a0ea4d39/packages/ai/src/providers/openai-codex.models.ts";
import { OPENAI_CODEX_GC_MODELS } from "/home/osso/.worktrees/pi-delta-terra-5a0ea4d39/packages/ai/src/providers/openai-codex-gc.models.ts";
import { OPENAI_CODEX_TEAM_MODELS } from "/home/osso/.worktrees/pi-delta-terra-5a0ea4d39/packages/ai/src/providers/openai-codex-team.models.ts";

const catalogs = [OPENAI_CODEX_MODELS, OPENAI_CODEX_GC_MODELS, OPENAI_CODEX_TEAM_MODELS];
createInterface({ input: process.stdin }).on("line", () => {
    console.log(JSON.stringify(catalogs.map(catalog => catalog["gpt-6.1-terra"])));
});
