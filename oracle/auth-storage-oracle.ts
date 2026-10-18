// Oracle for `core/auth-storage.ts`: real AuthStorage / ReadOnlyAuthStorage operations on a file.
import { serve } from "./config-auth-oracle-support.ts";

type Request =
	| { id: unknown; op: "modify"; authPath: string; provider: string; credential: unknown }
	| { id: unknown; op: "delete"; authPath: string; provider: string }
	| { id: unknown; op: "read"; authPath: string; provider: string; readOnly?: boolean }
	| { id: unknown; op: "list"; authPath: string; readOnly?: boolean };

type AuthApi = {
	AuthStorage: {
		create(authPath: string): {
			read(provider: string): Promise<unknown>;
			list(): Promise<unknown>;
			modify(provider: string, fn: (current: unknown) => Promise<unknown>): Promise<unknown>;
			delete(provider: string): Promise<void>;
		};
	};
	ReadOnlyAuthStorage: new (authPath: string) => {
		read(provider: string): Promise<unknown>;
		list(): Promise<unknown>;
	};
};

async function main(): Promise<void> {
	const { readFileSync, existsSync } = await import("node:fs");
	const { pathToFileURL } = await import("node:url");
	const sourcePath = process.argv[2];
	if (!sourcePath) throw new Error("auth storage oracle requires a source module path");
	const api = (await import(pathToFileURL(sourcePath).href)) as AuthApi;

	const fileContent = (path: string): string | null =>
		existsSync(path) ? readFileSync(path, "utf-8") : null;

	serve(async (raw) => {
		const request = raw as Request;
		const storage = api.AuthStorage.create(request.authPath);
		if (request.op === "read") {
			const target = request.readOnly ? new api.ReadOnlyAuthStorage(request.authPath) : storage;
			return { credential: (await target.read(request.provider)) ?? null };
		}
		if (request.op === "list") {
			const target = request.readOnly ? new api.ReadOnlyAuthStorage(request.authPath) : storage;
			return { entries: await target.list(), file: fileContent(request.authPath) };
		}
		if (request.op === "modify") {
			const credential = await storage.modify(request.provider, async () => request.credential);
			return { credential: credential ?? null, file: fileContent(request.authPath) };
		}
		await storage.delete(request.provider);
		return { file: fileContent(request.authPath) };
	});
}

void main().catch((error: unknown) => {
	process.stderr.write(`auth storage oracle failed: ${(error as Error).message}\n`);
	process.exitCode = 1;
});
