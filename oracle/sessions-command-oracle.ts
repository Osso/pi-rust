import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type SessionsApi = {
	handleSessionsCommand(args: string[], effects: Record<string, unknown>): Promise<boolean>;
};

void runOracle(async (module, request) => {
	const api = module as unknown as SessionsApi;
	const stdout: string[] = [];
	const stderr: string[] = [];
	process.exitCode = undefined;
	const handled = await api.handleSessionsCommand(request.args as string[], {
		stdout: (text: string) => stdout.push(text),
		stderr: (text: string) => stderr.push(text),
		controlDbPath: "/tmp/injected-control.sqlite",
		now: () => new Date(1783641600000),
		refreshMetadata: async () => {},
		archiveOlderThan: () => request.archived ?? [],
		listArchivedSessions: () => request.sessions ?? [],
		compressArchivedSession: (_db: string, path: string) => {
			if (path.includes("broken")) throw new Error("compression failed");
			return `${path}.zst`;
		},
		truncateToolOutput: () => request.report,
		agentDir: "/tmp/injected-agent",
	});
	const exitCode = process.exitCode ?? null;
	process.exitCode = 0;
	return { handled, exitCode, stdout, stderr };
}).catch((error: unknown) => {
	console.error("sessions-command oracle failed:", error);
	process.exitCode = 1;
});
