import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { createInterface } from "node:readline";

createInterface({ input: process.stdin }).on("line", (line) => {
	const { root } = JSON.parse(line) as { root: string };
	const input = "é-\ud800-A\u030a.txt";
	const resolved = resolve(root, input.normalize("NFD"));
	writeFileSync(resolved, "native path file\n");
	process.stdout.write(`${JSON.stringify({ input, resolved })}\n`);
});
