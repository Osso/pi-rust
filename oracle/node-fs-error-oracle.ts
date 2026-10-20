import * as fs from "node:fs";
import { createInterface } from "node:readline";

// Builtin Node filesystem failures, intentionally independent of application wrappers.
createInterface({ input: process.stdin }).on("line", async (line) => {
	const { operation, path } = JSON.parse(line);
	try {
		switch (operation) {
			case "read": fs.readFileSync(path); break;
			case "readAsync": await fs.promises.readFile(path); break;
			case "readFd": {
				const fd = fs.openSync(path, "r");
				try { fs.readSync(fd, Buffer.alloc(1), 0, 1, null); }
				finally { fs.closeSync(fd); }
				break;
			}
			case "write": fs.writeFileSync(path, "fixture"); break;
			case "mkdir": fs.mkdirSync(path); break;
			default: throw new Error(`Unknown fixture operation: ${operation}`);
		}
		console.log(JSON.stringify({ success: true }));
	} catch (error) {
		const e = error as NodeJS.ErrnoException;
		console.log(JSON.stringify({
			name: e.name,
			message: e.message,
			own: Object.fromEntries(Object.getOwnPropertyNames(e)
				.filter((key) => key !== "stack")
				.map((key) => [key, (e as unknown as Record<string, unknown>)[key]])),
		}));
	}
});
