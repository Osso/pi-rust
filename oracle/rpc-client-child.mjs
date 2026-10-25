import { createInterface } from "node:readline";
const reply = JSON.parse(process.env.PI_RPC_CLIENT_REPLY);
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
