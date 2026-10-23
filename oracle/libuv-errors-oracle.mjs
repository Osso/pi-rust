import { createInterface } from "node:readline";
import { getSystemErrorMap, getSystemErrorName, getSystemErrorMessage } from "node:util";

createInterface({ input: process.stdin }).on("line", (line) => {
	const request = JSON.parse(line);
	const errors = [...getSystemErrorMap()].sort((left, right) => left[0] - right[0]);
	const probes = request.codes.map((input) => {
		const code = input > 0 ? -input : input;
		return { input, code, name: getSystemErrorName(code), description: getSystemErrorMessage(code) };
	});
	console.log(JSON.stringify({ node: process.versions.node, libuv: process.versions.uv, errors, probes }));
});
