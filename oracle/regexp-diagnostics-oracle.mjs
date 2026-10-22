import { createInterface } from "node:readline";

for await (const line of createInterface({ input: process.stdin })) {
	const request = JSON.parse(line);
	const results = request.cases.map(({ units, flags }) => {
		let pattern = "";
		for (let start = 0; start < units.length; start += 8192) {
			pattern += String.fromCharCode(...units.slice(start, start + 8192));
		}
		try {
			new RegExp(pattern, flags);
			return { accepted: true };
		} catch (error) {
			return { accepted: false, name: error.name, units: Array.from({ length: error.message.length }, (_, index) => error.message.charCodeAt(index)) };
		}
	});
	process.stdout.write(`${JSON.stringify({ id: request.id, ok: true, results, node: process.version, v8: process.versions.v8, executable: process.execPath })}\n`);
}
