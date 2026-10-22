import { mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { createInterface } from "node:readline";

createInterface({ input: process.stdin }).on("line", (line) => {
	const request = JSON.parse(line) as { root: string; probe?: boolean };
	if (request.probe) {
		process.stdout.write(`${JSON.stringify({ ready: true })}\n`);
		return;
	}
	const { root } = request;
	const input = "é-\ud800-A\u030a.txt";
	const resolved = resolve(root, input.normalize("NFD"));
	writeFileSync(resolved, "native path file\n");
	const prompt = resolve(root, "prompt-\ud800.md");
	const skillRoot = resolve(root, "skill-\udfff");
	mkdirSync(skillRoot, { recursive: true });
	writeFileSync(prompt, "---\ndescription: prompt\n---\nbody\n");
	writeFileSync(resolve(skillRoot, "SKILL.md"), "---\ndescription: skill\n---\nbody\n");
	process.stdout.write(`${JSON.stringify({ input, resolved, prompt, skillRoot })}\n`);
});
