import { closeSync } from "node:fs";
import { createInterface } from "node:readline";

const reply = JSON.parse(process.env.PI_RPC_CLIENT_REPLY);
if (process.env.PI_RPC_STARTUP_SIGNAL)
	process.kill(process.pid, process.env.PI_RPC_STARTUP_SIGNAL);
if (process.env.PI_RPC_WRITING === "1") {
	process.on("SIGTERM", () => {});
	process.stdout.on("error", (error) => {
		process.stderr.write(`stdout-error:${error.code}\n`, () =>
			process.exit(42),
		);
	});
	setInterval(() => process.stdout.write('{"type":"pulse"}\n'), 10);
}
if (process.env.PI_RPC_BLOCK_INPUT === "close") {
	process.stdin.once("data", () => {
		process.stdin.pause();
		closeSync(0);
		process.stdout.write('{"type":"stdin_closed"}\n');
	});
	process.stdin.resume();
	setInterval(() => {}, 1000);
} else if (process.env.PI_RPC_BLOCK_INPUT === "1") setInterval(() => {}, 1000);
else
	createInterface({ input: process.stdin }).on("line", (line) => {
		if (process.env.PI_RPC_DELAY_ACK === "1")
			setTimeout(() => respond(line), 100);
		else respond(line);
	});

function respond(line) {
	if (process.env.PI_RPC_EXIT_ON_LINE === "1") process.exit(9);
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
}
