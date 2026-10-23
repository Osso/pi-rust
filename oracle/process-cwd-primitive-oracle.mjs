import { readFileSync, rmdirSync } from "node:fs";

if (process.version !== "v26.11.1")
	throw new Error(
		`Node cwd reference requires v26.11.1, got ${process.version}`,
	);
const request = JSON.parse(readFileSync(process.argv[2], "utf8"));
const units = (value) =>
	Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
const errorWire = (error) => ({
	ok: false,
	error: {
		name: error.name,
		message: units(error.message),
		code: error.code ?? null,
		errno: error.errno ?? null,
		syscall: error.syscall ?? null,
		path: error.path === undefined ? null : units(error.path),
		dest: error.dest === undefined ? null : units(error.dest),
	},
});
const results = request.operations.map((operation) => {
	try {
		if (operation.kind === "cwd")
			return { ok: true, cwd: units(process.cwd()) };
		const path = String.fromCharCode(...operation.path);
		if (operation.kind === "chdir") process.chdir(path);
		else if (operation.kind === "remove") rmdirSync(path);
		else throw new Error(`Unsupported operation: ${operation.kind}`);
		return { ok: true };
	} catch (error) {
		if (!(error instanceof Error)) throw error;
		return errorWire(error);
	}
});
process.chdir(request.base);
console.log(JSON.stringify(results));
