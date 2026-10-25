// Differential oracle for coding-agent HTML export.
// argv[2] is the pinned `packages/coding-agent/src` directory; each request names
// the module (relative to it) and an operation handled below.
import { readFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

type Api = Record<string, (...args: unknown[]) => unknown> &
	Record<string, unknown>;
type Request = {
	id: number;
	module: string;
	operation: string;
	[key: string]: unknown;
};
type Handler = (api: Api, request: Request) => unknown | Promise<unknown>;

function errorOf(error: unknown): { name: string; message: string } {
	return error instanceof Error
		? { name: error.name, message: error.message }
		: { name: "Unknown", message: String(error) };
}

const handlers: Record<string, Handler> = {
	exportPaths: async (api, request) => {
		const { SessionManager } = await import(
			pathToFileURL(`${process.argv[2]}/core/session-manager.ts`).href
		);
		const oldCwd = process.cwd();
		process.chdir(request.cwd as string);
		try {
			const manager = SessionManager.openReadOnly(request.inputPath as string);
			const options =
				request.outputPath === undefined
					? undefined
					: request.shorthand
						? request.outputPath
						: { outputPath: request.outputPath };
			const outputPath = request.live
				? await api.exportSessionToHtml(manager, undefined, options)
				: await api.exportFromFile(request.inputPath, options);
			return {
				path: outputPath,
				html: readFileSync(outputPath as string, "utf-8"),
			};
		} catch (error) {
			return { error: errorOf(error) };
		} finally {
			process.chdir(oldCwd);
		}
	},
	exportLive: async (api, request) => {
		const { SessionManager } = await import(
			pathToFileURL(`${process.argv[2]}/core/session-manager.ts`).href
		);
		const calls: unknown[] = [];
		const toolRenderer = {
			renderCall(id: string, name: string, args: unknown) {
				calls.push({ id, name, args });
				return request.callHtml;
			},
			renderResult() {
				return undefined;
			},
		};
		await api.exportSessionToHtml(
			SessionManager.openReadOnly(request.inputPath as string),
			undefined,
			{ outputPath: request.outputPath, toolRenderer },
		);
		return { html: readFileSync(request.outputPath as string, "utf-8"), calls };
	},
	exportFromFile: async (api, request) => {
		try {
			await api.exportFromFile(request.inputPath as string, {
				outputPath: request.outputPath as string,
				themeName: request.themeName as string | undefined,
			});
			return {
				ok: true,
				html: readFileSync(request.outputPath as string, "utf-8"),
			};
		} catch (error) {
			return { ok: false, error: errorOf(error).message };
		}
	},
};

async function main(): Promise<void> {
	const sourceRoot = process.argv[2];
	if (!sourceRoot)
		throw new Error("export oracle requires the coding-agent src directory");
	const modules = new Map<string, Api>();
	const load = async (module: string): Promise<Api> => {
		let api = modules.get(module);
		if (!api) {
			api = (await import(
				pathToFileURL(`${sourceRoot}/${module}`).href
			)) as Api;
			modules.set(module, api);
		}
		return api;
	};
	for await (const line of createInterface({ input: process.stdin })) {
		let id: number | null = null;
		try {
			const request = JSON.parse(line) as Request;
			id = request.id;
			const handler = handlers[request.operation];
			if (!handler) throw new Error(`unknown operation ${request.operation}`);
			const output = await handler(await load(request.module), request);
			process.stdout.write(`${JSON.stringify({ id, ok: true, output })}\n`);
		} catch (error) {
			process.stdout.write(
				`${JSON.stringify({ id, ok: false, error: errorOf(error) })}\n`,
			);
		}
	}
}

void main().catch((error: unknown) => {
	process.stderr.write(`export oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
