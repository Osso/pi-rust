import { createInterface } from "node:readline";
const reply = JSON.parse(process.env.PI_RPC_CLIENT_REPLY);
if (process.env.PI_RPC_WRITING === "1") {
	process.on("SIGTERM", () => {});
	process.stdout.on("error", (error) => {
		process.stderr.write(`stdout-error:${error.code}\n`, () =>
			process.exit(42),
		);
	});
	setInterval(() => process.stdout.write('{"type":"pulse"}\n'), 10);
}
createInterface({ input: process.stdin }).on("line", (line) => {
	const command = JSON.parse(line);
	for (const event of JSON.parse(process.env.PI_RPC_EXTRA_EVENTS ?? "[]"))
		process.stdout.write(`${JSON.stringify(event)}\n`);
	process.stdout.write(`${JSON.stringify({ id: command.id, ...reply })}\n`);
	process.stdout.write(
		`${JSON.stringify({ type: "agent_end", willRetry: true, messages: [] })}\n`,
	);
	setTimeout(
		() => process.stdout.write(`${JSON.stringify({ type: "later" })}\n`),
		20,
	);
	setTimeout(
		() =>
			process.stdout.write(
				`${JSON.stringify({ type: "agent_end", messages: [] })}\n`,
			),
		40,
	);
});
