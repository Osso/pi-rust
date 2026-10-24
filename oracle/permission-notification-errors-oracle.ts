import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {
	createPermissionPromptHandler(options: {
		permissionPromptTool: string;
		cwd: string;
		callTool(): Promise<unknown>;
		desktopNotifier(): { close(): void };
	}): (event: unknown) => Promise<unknown>;
};

void runOracle(async (module, request) => {
	let calls = 0;
	let sends = 0;
	let closes = 0;
	const logs: unknown[][] = [];
	const previous = console.error;
	console.error = (...values: unknown[]) => {
		logs.push(values);
	};
	try {
		const handler = (module as Api).createPermissionPromptHandler({
			permissionPromptTool: "mcp__approval__prompt",
			cwd: "/repo",
			callTool: async () => {
				calls++;
				return { behavior: "deny", message: "denied" };
			},
			desktopNotifier: () => {
				sends++;
				if (request.phase === "send") throw request.reason;
				return {
					close: () => {
						closes++;
						throw request.reason;
					},
				};
			},
		});
		const decision = await handler({
			toolName: "bash",
			toolCallId: "fixture",
			input: {},
		});
		return { decision, calls, sends, closes, logs };
	} finally {
		console.error = previous;
	}
}).catch((error: unknown) => {
	process.stderr.write(
		`permission notification oracle failed: ${String(error)}\n`,
	);
	process.exitCode = 1;
});
