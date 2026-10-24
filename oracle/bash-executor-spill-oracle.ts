import { createHash } from "node:crypto";
import {
	existsSync,
	readdirSync,
	readFileSync,
	statSync,
	unlinkSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Input = {
	chunks: number[][];
	reenter?: boolean;
	error?: string;
	chunkError?: string;
	env?: Record<string, string>;
	fault?: string;
	expectFile?: boolean;
	late?: boolean;
};
type Result = { output: string; fullOutputPath?: string };
type Api = {
	executeBashWithOperations(
		command: string,
		cwd: string,
		operations: unknown,
		options: unknown,
	): Promise<Result>;
};
const environmentKeys = ["TMPDIR", "TMP", "TEMP", "PI_BASH_SPILL_FAIL"];
const describe = (text: string) => ({
	length: text.length,
	digest: createHash("sha256").update(text).digest("hex"),
});
const spillNames = (directory: string) =>
	existsSync(directory)
		? readdirSync(directory).filter((name) =>
				/^pi-bash-[0-9a-f]{16}\.log$/.test(name),
			)
		: [];
async function waitForFile(path: string, size: number): Promise<void> {
	const deadline = Date.now() + 3000;
	while (!existsSync(path) || statSync(path).size !== size) {
		if (Date.now() >= deadline)
			throw new Error(`spill bytes did not finish: ${path}`);
		await new Promise((resolve) => setTimeout(resolve, 5));
	}
}
function describeUncaught(
	error: Error & {
		code?: string;
		errno?: number;
		syscall?: string;
		path?: string;
	},
	chunks: string[],
): unknown {
	return {
		uncaught: {
			name: error.name,
			message: error.message,
			...(error.code === undefined ? {} : { code: error.code }),
			...(error.errno === undefined ? {} : { errno: error.errno }),
			...(error.syscall === undefined ? {} : { syscall: error.syscall }),
			...(error.path === undefined ? {} : { path: error.path }),
		},
		constructor: error.constructor.name,
		string: String(error),
		chunks: chunks.map(describe),
	};
}
async function snapshotSpill(
	result: unknown,
	chunks: string[],
	before: Set<string>,
	expectFile: boolean | undefined,
): Promise<unknown> {
	let names = spillNames(tmpdir()).filter((name) => !before.has(name));
	if (expectFile) {
		const deadline = Date.now() + 3000;
		while (names.length !== 1) {
			if (Date.now() >= deadline)
				throw new Error("expected spill file was not created");
			await new Promise((resolve) => setTimeout(resolve, 5));
			names = spillNames(tmpdir()).filter((name) => !before.has(name));
		}
	}
	const valueResult = result as { value?: { fullOutputPath?: string } };
	const path =
		valueResult.value?.fullOutputPath ??
		(names.length === 1 ? join(tmpdir(), names[0]) : undefined);
	let fullFile: unknown;
	if (path) {
		await waitForFile(path, Buffer.byteLength(chunks.join("")));
		const bytes = readFileSync(path);
		fullFile = {
			size: bytes.length,
			digest: createHash("sha256").update(bytes).digest("hex"),
			mode: statSync(path).mode & 0o777,
			directory: dirname(path),
			randomName: /^pi-bash-[0-9a-f]{16}\.log$/.test(basename(path)),
		};
	}
	return { result, chunks: chunks.map(describe), fullFile };
}
async function runCase(api: Api, input: Input): Promise<unknown> {
	const before = new Set(spillNames(tmpdir()));
	const chunks: string[] = [];
	let onData: (data: Buffer) => void = () => {
		throw new Error("operation has not started");
	};
	let reentered = false;
	let observeUncaught: (
		error: Error & {
			code?: string;
			errno?: number;
			syscall?: string;
			path?: string;
		},
	) => void = () => {};
	let timeout: NodeJS.Timeout | undefined;
	const uncaught = new Promise((resolve) => {
		observeUncaught = (error) => resolve(describeUncaught(error, chunks));
		process.once("uncaughtException", observeUncaught);
		timeout = setTimeout(() => resolve({ oracleTimeout: true }), 3000);
	});
	try {
		let result: unknown;
		try {
			const value = await api.executeBashWithOperations(
				"spill",
				"/cwd",
				{
					exec: async (
						_command: string,
						_cwd: string,
						options: { onData(data: Buffer): void },
					) => {
						onData = options.onData;
						for (const bytes of input.chunks) onData(Buffer.from(bytes));
						if (input.error) throw new Error(input.error);
						return { exitCode: 0 };
					},
				},
				{
					onChunk: (text: string) => {
						chunks.push(text);
						if (input.reenter && !reentered) {
							reentered = true;
							onData(Buffer.from("nested"));
						}
						if (input.chunkError !== undefined)
							throw new Error(input.chunkError);
					},
				},
			);
			result = {
				ok: true,
				value: { ...value, output: describe(value.output) },
			};
		} catch (error) {
			if (!(error instanceof Error)) throw error;
			result = { ok: false, error: error.message };
		}
		if (input.late) onData(Buffer.from("late"));
		if (input.fault) return await uncaught;
		return await snapshotSpill(result, chunks, before, input.expectFile);
	} finally {
		process.removeListener("uncaughtException", observeUncaught);
		if (timeout) clearTimeout(timeout);
		delete process.env.PI_BASH_SPILL_FAIL;
		for (const name of spillNames(tmpdir()).filter((name) => !before.has(name)))
			unlinkSync(join(tmpdir(), name));
	}
}
void runOracle(async (module, request) => {
	const output = [];
	for (const value of requireCases(request)) {
		const input = value as Input;
		const previous = environmentKeys.map((key) => process.env[key]);
		try {
			if (input.env) {
				for (const key of ["TMPDIR", "TMP", "TEMP"]) delete process.env[key];
				Object.assign(process.env, input.env);
			}
			if (input.fault && input.fault !== "open")
				process.env.PI_BASH_SPILL_FAIL = input.fault;
			output.push(await runCase(module as Api, input));
		} finally {
			for (const [index, key] of environmentKeys.entries()) {
				if (previous[index] === undefined) delete process.env[key];
				else process.env[key] = previous[index];
			}
		}
	}
	return output;
}).catch((error: unknown) => {
	process.stderr.write(`bash executor spill oracle failed: ${String(error)}\n`);
	process.exitCode = 1;
});
