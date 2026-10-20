import { createInterface } from "node:readline";

async function main() {
	// Load the real, source-lock-verified codec supplied by the shared Rust harness.
	const { parseHeader, parseMutation } = await import(process.argv[2]);
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		const result = request.operation === "header" ? parseHeader(request.line) : parseMutation(request.line);
		process.stdout.write(`${JSON.stringify(result.ok
			? result
			: { ok: false, error: { kind: result.error.kind, message: result.error.message } })}\n`);
	}
}

main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
