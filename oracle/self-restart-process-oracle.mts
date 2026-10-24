import { EventEmitter } from "node:events";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const {
	restartCurrentProcess,
	spawnSelfRestart,
	ENV_SELF_RESTART_REQUEST,
	ENV_SELF_RESTART_SESSION,
	ENV_SELF_RESTART_PROMPT,
	ENV_SELF_RESTART_OLD_PID,
} = await import(pathToFileURL(`${source}/core/self-restart.ts`).href);
interface Request {
	operation: string;
	kind: "native" | "raw";
	sessionFile: string;
	prompt?: string;
	oldPid?: number;
	argv: string[];
	argv0: string;
	execArgv: string[];
	value: { message: string; code: string; custom: unknown };
}
function errorWire(error: unknown) {
	if (error instanceof Error) {
		const native = error as Error & { code?: unknown; custom?: unknown };
		return {
			kind: "error",
			class: error instanceof RangeError ? "RangeError" : "Error",
			value: {
				name: native.name,
				message: native.message,
				...(native.code === undefined ? {} : { code: native.code }),
				...(native.custom === undefined ? {} : { custom: native.custom }),
			},
			string: String(error),
		};
	}
	return { kind: "value", value: error };
}
function envWire(env: NodeJS.ProcessEnv) {
	const pid = env[ENV_SELF_RESTART_OLD_PID];
	return {
		marker: env[ENV_SELF_RESTART_REQUEST],
		session: env[ENV_SELF_RESTART_SESSION],
		prompt: env[ENV_SELF_RESTART_PROMPT],
		oldPid: pid === String(process.pid) ? "SELF" : pid,
	};
}
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line) as Request;
		const failure =
			request.kind === "native"
				? Object.assign(new RangeError(request.value.message), {
						code: request.value.code,
						custom: request.value.custom,
					})
				: request.value;
		const input = {
			sessionFile: request.sessionFile,
			prompt: request.prompt,
			oldPid: request.oldPid,
		};
		const calls: unknown[] = [];
		let result: number | undefined;
		let error: unknown;
		try {
			if (request.operation.startsWith("helper-")) {
				result = await spawnSelfRestart(input, {
					argv: request.argv,
					argv0: request.argv0,
					execArgv: request.execArgv,
					waitForExit: request.operation.includes("nowait") ? false : undefined,
					spawn: (
						file: string,
						args: string[],
						options: { cwd: string; env: NodeJS.ProcessEnv; stdio: string },
					) => {
						calls.push({
							op: "spawn",
							selfExecutable: file === process.execPath,
							args,
							cwdMatches: options.cwd === process.cwd(),
							env: envWire(options.env),
							stdio: options.stdio,
						});
						if (request.operation === "helper-spawn-throw") throw failure;
						const child = new EventEmitter() as EventEmitter & {
							unref?: () => void;
						};
						if (request.operation !== "helper-nowait-no-unref")
							child.unref = () => {
								calls.push({ op: "unref" });
								if (request.operation === "helper-nowait-unref-throw")
									throw failure;
							};
						setImmediate(() => {
							calls.push({
								op:
									request.operation === "helper-error"
										? "errorEvent"
										: "exitEvent",
							});
							if (request.operation === "helper-error")
								child.emit("error", failure);
							else
								child.emit(
									"exit",
									request.operation === "helper-null" ? null : 17,
								);
						});
						return child;
					},
				});
			} else {
				await restartCurrentProcess(input, {
					argv: request.argv,
					argv0: request.argv0,
					execArgv: request.execArgv,
					execve: request.operation.startsWith("exec-")
						? (file: string, args: string[], env: NodeJS.ProcessEnv) => {
								calls.push({
									op: "exec",
									selfExecutable: file === process.execPath,
									args,
									env: envWire(env),
								});
								if (request.operation === "exec-throw") throw failure;
							}
						: undefined,
					spawnSelfRestart: async (
						input: { sessionFile: string; prompt?: string; oldPid?: number },
						dependencies: {
							waitForExit?: boolean;
							argv?: string[];
							argv0?: string;
							execArgv?: string[];
						},
					) => {
						calls.push({
							op: "spawnRestart",
							request: {
								sessionFile: input.sessionFile,
								prompt: input.prompt,
								oldPidIsSelf: input.oldPid === process.pid,
							},
							options: {
								waitForExit: dependencies.waitForExit,
								hasArgv: dependencies.argv !== undefined,
								hasArgv0: dependencies.argv0 !== undefined,
								hasExecArgv: dependencies.execArgv !== undefined,
							},
						});
						if (request.operation === "spawn-throw") throw failure;
						return 17;
					},
					exit: (code: number) => {
						calls.push({ op: "exit", code });
						if (request.operation === "exit-throw") throw failure;
						return undefined as never;
					},
				});
			}
		} catch (caught) {
			error = caught;
		}
		console.log(
			JSON.stringify({
				calls,
				...(result === undefined ? {} : { result }),
				...(error === undefined ? {} : { error: errorWire(error) }),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ harnessError: String(error) }));
	}
}
