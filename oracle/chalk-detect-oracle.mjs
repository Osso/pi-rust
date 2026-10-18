import { pathToFileURL } from "node:url";

function readModulePath() {
	const modulePath = process.argv[2];
	if (modulePath === undefined) {
		throw new Error("chalk detection oracle requires a supports-color module path");
	}
	return modulePath;
}

async function main() {
	const { createSupportsColor } = await import(pathToFileURL(readModulePath()).href);
	const level = (isTTY) => {
		const detected = createSupportsColor({ isTTY });
		return detected === false ? 0 : detected.level;
	};
	process.stdout.write(`${JSON.stringify({ tty: level(true), noTty: level(false) })}\n`);
}

void main().catch((error) => {
	process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
	process.exitCode = 1;
});
