import { pathToFileURL } from "node:url";
import { importSource, serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const api = await importSource<any>();
	const db = await import(new URL("../core/session-control-db.ts", pathToFileURL(process.argv[2])).href);
	serve(async (request) => {
		const controller = new AbortController();
		if (request.preAbort) controller.abort("custom reason");
		const input = { ...request.input as object, signal: controller.signal } as any;
		if (request.timeoutTag) input.timeoutMs = Number(request.timeoutTag);
		let startups = 0;
		let timer: ReturnType<typeof setTimeout> | undefined;
		if (request.abortLater) timer = setTimeout(() => controller.abort("later reason"), 10);
		let outcome;
		try {
			outcome = { result: await api.requestSupervisorDecision(input, { ensureRunning: async () => {
				startups++;
				if (request.abortStartup) controller.abort("startup reason");
				if (request.startupError) throw new Error(request.startupError as string);
			} }) };
		} catch (error) {
			outcome = { error: { name: (error as Error).name, message: (error as Error).message } };
		} finally { if (timer) clearTimeout(timer); }
		const requests = [];
		for (let id = 1; ; id++) {
			const row = db.readSupervisorRequest(input.controlDbPath, id);
			if (!row) break;
			requests.push({ status: row.status, ...(row.response ? { response: row.response } : {}) });
		}
		return { ...outcome, startups, requests };
	});
}
void main();
