import { importSource, serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const api = await importSource<any>();
	serve(async (request) => {
		if (request.op === "format") return api.formatCodexRestrictionValidation(request.original, request.failure, request.proposal);
		if (request.op === "validate") return api.parseCodexRestrictionValidation(request.message, request.proposal, request.original);
		const controller = new AbortController();
		if (request.preAbort) controller.abort("caller reason");
		const calls: unknown[] = [];
		const result = await api.requestCodexRestrictionRescope({
			controlDbPath: "/tmp/recovery-test.db", senderSessionId: "session", cwd: "/tmp", ...request.options,
			signal: controller.signal,
			requester: async (input: any) => {
				const { signal, ...fields } = input;
				calls.push({ ...fields, signalForwarded: signal === controller.signal });
				if (request.abortDuring) controller.abort("during request");
				if (request.throwReason !== undefined) throw new Error(request.throwReason as string);
				return request.response;
			},
		});
		return { result, calls };
	});
}
void main();
