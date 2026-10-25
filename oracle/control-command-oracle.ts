import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type ControlApi = {
	handleControlCommand(args: string[], effects: {
		stdout(text: string): void;
		stderr(text: string): void;
		signalProcess?: (pid: number, signal: string) => void;
	}): boolean;
};

void runOracle((module, request) => {
	const api = module as unknown as ControlApi;
	const stdout: string[] = [];
	const stderr: string[] = [];
	const signals: [number, string][] = [];
	process.exitCode = undefined;
	if (request.nativeSignal === true) {
		process.env.PI_CODING_AGENT_STATE_DIR = String(request.stateDir);
		return api.handleControlCommand(request.args as string[], {
			stdout: (text) => stdout.push(text),
			stderr: (text) => stderr.push(text),
		});
	}
	const handled = api.handleControlCommand(request.args as string[], {
		stdout: (text) => stdout.push(text),
		stderr: (text) => stderr.push(text),
		signalProcess: (pid, signal) => signals.push([pid, signal]),
	});
	const exitCode = process.exitCode ?? null;
	process.exitCode = 0;
	return { handled, exitCode, stdout, stderr, signals };
}).catch((error: unknown) => {
	console.error("control-command oracle failed:", error);
	process.exitCode = 1;
});
