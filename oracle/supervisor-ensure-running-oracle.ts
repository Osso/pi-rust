import { importSource, serve } from "./config-auth-oracle-support.ts";

async function main(): Promise<void> {
	const api = await importSource<any>();
	serve(async (request) => {
		if (request.op === "invocation") return api.resolveSupervisorLaunchInvocation(request.runtime);
		if (request.op === "probeError") {
			const properties = request.error as Record<string, unknown>;
			const error = Object.assign(new Error(properties.message as string), properties);
			try {
				await api.ensureSupervisorRunning({ controlDbPath: "/unused" }, { probe: async () => { throw error; } });
			} catch (caught) {
				const value = caught as typeof error;
				return { name: value.name, message: value.message, code: value.code, info: value.info, errno: value.errno, syscall: value.syscall };
			}
			throw new Error("expected startup probe failure");
		}
		if (request.op !== "scenario") return api.getSupervisorStartLockPath(request.path);
		let resident = request.initial;
		let launches = 0;
		let terminations = 0;
		try {
			const result = await api.ensureSupervisorRunning(request.input, {
				probe: async () => resident,
				launch: async () => {
					launches++;
					if (request.launchError) throw new Error(request.launchError as string);
					resident = request.afterLaunch;
				},
				terminate: async () => { terminations++; if (!request.keepOld) resident = undefined; },
			});
			return { result, launches, terminations };
		} catch (error) {
			return { error: (error as Error).message, launches, terminations };
		}
	});
}
void main();
