import { readFileSync, rmSync, statSync } from "node:fs";
import { basename, dirname } from "node:path";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type Result = {
	content: { type: string; text: string }[];
	details?: { fullOutputPath?: string };
};
type FileResult = { ok: true; value: string } | { ok: false; error: unknown };
type Env = Record<string, string>;
type Output = {
	append(data: Buffer): void;
	finish(): void;
	snapshot(options?: { persistIfTruncated: boolean }): {
		fullOutputPath?: string;
	};
	closeTempFile(): Promise<void>;
};
type NodeEnv = {
	createTempDir(prefix: string): Promise<FileResult>;
	createTempFile(): Promise<FileResult>;
};
type Wrap = (definition: {
	name: string;
	label: string;
	description: string;
	parameters: object;
	execute(): Promise<Result>;
}) => { execute(id: string, parameters: object): Promise<Result> };
function setEnvironment(env: Env): void {
	for (const key of ["TMPDIR", "TMP", "TEMP"]) delete process.env[key];
	Object.assign(process.env, env);
}
function readFileSnapshot(
	path: string,
	prefix: string,
): { root: string; bytes: string; name: boolean } {
	return {
		root: dirname(path),
		bytes: readFileSync(path).toString("hex"),
		name: new RegExp(`^${prefix}-[0-9a-f]{16}\\.log$`).test(basename(path)),
	};
}
async function probeAccumulator(
	api: Record<string, unknown>,
	env: Env,
): Promise<unknown> {
	const Constructor = api.OutputAccumulator as new (options: {
		maxBytes: number;
	}) => Output;
	const output = new Constructor({ maxBytes: 1 });
	setEnvironment(env);
	output.append(Buffer.from("payload"));
	output.finish();
	const path = output.snapshot().fullOutputPath;
	if (!path) throw new Error("missing accumulator spill");
	await output.closeTempFile();
	setEnvironment({ TEMP: String(env.TMPDIR || env.TMP || env.TEMP) });
	const afterCloseSamePath =
		output.snapshot({ persistIfTruncated: true }).fullOutputPath === path;
	try {
		return {
			...readFileSnapshot(path, "pi-output"),
			afterCloseSamePath,
		};
	} finally {
		rmSync(path);
	}
}
async function probeNodeEnvironment(
	api: Record<string, unknown>,
	env: Env,
): Promise<unknown> {
	const Constructor = api.NodeExecutionEnv as new (options: {
		cwd: string;
	}) => NodeEnv;
	const filesystem = new Constructor({ cwd: "/cwd" });
	setEnvironment(env);
	const directory = await filesystem.createTempDir("probe-");
	const file = await filesystem.createTempFile();
	if (!directory.ok || !file.ok)
		throw new Error("temporary filesystem operation failed");
	try {
		return {
			root: dirname(directory.value),
			directory: statSync(directory.value).isDirectory(),
			name: /^probe-[A-Za-z0-9]{6}$/.test(basename(directory.value)),
			fileRoot: dirname(dirname(file.value)),
			fileBytes: readFileSync(file.value).toString("hex"),
		};
	} finally {
		rmSync(directory.value, { recursive: true });
		rmSync(dirname(file.value), { recursive: true });
	}
}
async function probeWrapper(
	api: Record<string, unknown>,
	env: Env,
): Promise<unknown> {
	const wrap = api.wrapToolDefinition as Wrap;
	const tool = wrap({
		name: "probe",
		label: "Probe",
		description: "Probe",
		parameters: {},
		execute: async () => ({
			content: [{ type: "text", text: "line\n".repeat(2500) }],
		}),
	});
	setEnvironment(env);
	const result = await tool.execute("call", {});
	const path = result.details?.fullOutputPath;
	if (!path) throw new Error("missing wrapper spill");
	try {
		return readFileSnapshot(path, "pi-tool");
	} finally {
		rmSync(path);
	}
}
void runOracle(async (api, request) => {
	const env = request.env as Env;
	setEnvironment(request.before as Env);
	if (request.operation === "accumulator") return probeAccumulator(api, env);
	if (request.operation === "node") return probeNodeEnvironment(api, env);
	if (request.operation === "wrapper") return probeWrapper(api, env);
	throw new Error(`unknown temp consumer ${request.operation}`);
}).catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
