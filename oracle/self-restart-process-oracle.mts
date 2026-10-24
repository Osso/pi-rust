import { EventEmitter } from "node:events";
import { readFileSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const reference = JSON.parse(
	readFileSync(
		new URL("../docs/porting/node-execve-source-lock.json", import.meta.url),
		"utf8",
	),
) as { nodeVersion: string };
if (process.version !== reference.nodeVersion)
	throw new Error(
		`restart oracle requires ${reference.nodeVersion}, got ${process.version}`,
	);
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
	monitorFile?: string;
	resultCode?: string | null;
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
		const native = error as Error & {
			code?: unknown;
			custom?: unknown;
			context?: unknown;
			errno?: unknown;
			syscall?: unknown;
			path?: unknown;
			spawnargs?: unknown;
		};
		return {
			kind: "error",
			class:
				error instanceof RangeError
					? "RangeError"
					: error instanceof TypeError
						? "TypeError"
						: "Error",
			value: {
				name: native.name,
				message: native.message,
				...(native.code === undefined ? {} : { code: native.code }),
				...(native.custom === undefined ? {} : { custom: native.custom }),
				...(native.context === undefined ? {} : { context: native.context }),
				...(native.errno === undefined ? {} : { errno: native.errno }),
				...(native.syscall === undefined ? {} : { syscall: native.syscall }),
				...(native.path === undefined ? {} : { path: native.path }),
				...(native.spawnargs === undefined
					? {}
					: { spawnargs: native.spawnargs }),
			},
			string: String(error),
		};
	}
	return { kind: "value", value: error };
}
function numberFlags(code: number) {
	return {
		negativeZero: Object.is(code, -0),
		nan: Number.isNaN(code),
		positiveInfinity: code === Infinity,
		negativeInfinity: code === -Infinity,
	};
}
function resultCode(request: Request): number | null {
	return request.resultCode === undefined
		? request.operation === "helper-null"
			? null
			: 17
		: request.resultCode === null
			? null
			: Number(request.resultCode);
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
		if (request.monitorFile) {
			const file = request.monitorFile;
			process.once("uncaughtExceptionMonitor", (error) =>
				writeFileSync(file, JSON.stringify(errorWire(error))),
			);
		}
		if (request.operation === "environment-order") {
			const base = [
				["PI_RESTART_ENV_Z", "z"],
				[ENV_SELF_RESTART_PROMPT, "stale"],
				["PI_RESTART_ENV_A", "a"],
				[ENV_SELF_RESTART_REQUEST, "stale"],
				["10", "ten"],
				["2", "two"],
			] as const;
			const selected = new Set<string>([
				...base.map(([key]) => key),
				ENV_SELF_RESTART_SESSION,
				ENV_SELF_RESTART_OLD_PID,
			]);
			for (const key of selected) delete process.env[key];
			for (const [key, value] of base) process.env[key] = value;
			let entries: [string, string | undefined][] = [];
			try {
				await restartCurrentProcess(
					{
						sessionFile: request.sessionFile,
						prompt: request.prompt,
						oldPid: request.oldPid,
					},
					{
						execve: (
							_file: string,
							_args: string[],
							env: NodeJS.ProcessEnv,
						) => {
							entries = Object.entries(env)
								.filter(([key]) => selected.has(key))
								.map(([key, value]) => [
									key,
									key === ENV_SELF_RESTART_OLD_PID &&
									value === String(process.pid)
										? "SELF"
										: value,
								]);
						},
					},
				);
			} catch (error) {
				if (
					!(error instanceof Error) ||
					error.message !== "execve returned after replacing the process image"
				)
					throw error;
			}
			console.log(JSON.stringify(entries));
			continue;
		}
		if (request.operation.startsWith("real-")) {
			const hasFailure =
				request.argv.some(
					(argument) => argument.includes("\0") || argument.length > 131072,
				) || request.prompt?.includes("\0");
			if (!hasFailure) throw new Error("unsafe actual exec/spawn fixture");
			for (const key of Object.keys(process.env)) delete process.env[key];
			let error: unknown;
			const input = {
				sessionFile: request.sessionFile,
				prompt: request.prompt,
				oldPid: request.oldPid,
			};
			const dependencies = {
				argv: request.argv,
				argv0: request.argv0,
				execArgv: request.execArgv,
			};
			try {
				if (request.operation === "real-exec")
					await restartCurrentProcess(input, dependencies);
				else await spawnSelfRestart(input, dependencies);
			} catch (caught) {
				error = caught;
			}
			console.log(
				JSON.stringify({
					error: errorWire(error),
					executable: process.execPath,
				}),
			);
			continue;
		}
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
							const isError =
								request.operation === "helper-error" ||
								request.operation === "helper-nowait-error";
							calls.push({
								op: isError ? "errorEvent" : "exitEvent",
							});
							if (isError) child.emit("error", failure);
							else child.emit("exit", resultCode(request));
						});
						return child;
					},
				});
			} else if (request.operation === "default-exit") {
				await restartCurrentProcess(input, {
					execve: undefined,
					spawnSelfRestart: async () => Number(request.resultCode),
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
						return resultCode(request) ?? 0;
					},
					exit: (code: number) => {
						calls.push({
							op: "exit",
							code,
							...(request.resultCode === undefined
								? {}
								: { number: numberFlags(code) }),
						});
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
				...(result === undefined || request.resultCode === undefined
					? {}
					: { number: numberFlags(result) }),
				...(error === undefined ? {} : { error: errorWire(error) }),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ harnessError: String(error) }));
	}
}
