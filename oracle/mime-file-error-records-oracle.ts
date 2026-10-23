import { readSync } from "node:fs";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type FileError = Error & { code?: string; syscall?: string; errno?: number; path?: string };
function errorRecord(error: FileError) {
	return { name: error.name, message: error.message, code: error.code, syscall: error.syscall,
		...(error.errno === undefined ? {} : { errno: error.errno }),
		...(error.path === undefined ? {} : { path: error.path }) };
}
void runOracle(async (api, request) => {
	try {
		if (request.operation === "pipe-read") {
			readSync(0, Buffer.alloc(1), 0, 1, 0);
			return { unexpected: true };
		}
		const detect = api.detectSupportedImageMimeTypeFromFile as (path: string) => Promise<string | null>;
		return { mime: await detect(String(request.path)) };
	} catch (error) { return { error: errorRecord(error as FileError) }; }
}).catch((error: unknown) => { console.error(error); process.exitCode = 1; });
