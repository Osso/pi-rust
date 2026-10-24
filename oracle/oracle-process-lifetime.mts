// Test only the generic oracle transport's process/flag lifetime, not a product module.
import { createInterface } from "node:readline";
import vm from "node:vm";

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line) as { id: number; op: string };
	if (request.op !== "watchdog_probe")
		throw new Error(`Unknown operation: ${request.op}`);
	const output = {
		pid: process.pid,
		callerFlag: typeof vm.SourceTextModule === "function",
	};
	process.stdout.write(
		`${JSON.stringify({ id: request.id, ok: true, output })}\n`,
	);
}
