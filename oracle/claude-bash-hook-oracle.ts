// Differential oracle for the bundled `claude-bash-hook` extension.

import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type AnyRecord = Record<string, any>;

void runOracle(async (api, request) => {
	const cases = (request.cases as AnyRecord[]) ?? [];
	const review = api.reviewToolWithClaudeBashHook as (
		event: AnyRecord,
		cwd: string,
	) => Promise<AnyRecord>;
	const results: AnyRecord[] = [];
	for (const input of cases) {
		const dir = mkdtempSync(join(tmpdir(), "pi-claude-bash-hook-oracle-"));
		const script = join(dir, "hook.sh");
		writeFileSync(script, `#!/bin/sh\ncat > /dev/null\nprintf '%s' '${input.output ?? ""}'\n`);
		chmodSync(script, 0o755);
		process.env.PI_CLAUDE_BASH_HOOK = script;
		const event = {
			type: "tool_call",
			toolName: input.toolName,
			toolCallId: "test-call",
			bypassPermissions: false,
			input: input.input,
		};
		const output: AnyRecord = { name: input.name };
		try {
			output.result = await review(event, input.cwd ?? "/repo");
		} catch (error) {
			output.error = error instanceof Error ? error.message : String(error);
		}
		results.push(output);
	}
	return { results };
}).catch((error: unknown) => {
	process.stderr.write(`claude-bash-hook oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
