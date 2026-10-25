import {
	readFileSync,
	writeFileSync,
	mkdtempSync,
	mkdirSync,
	rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";
import { createInterface } from "node:readline";

async function main() {
	const source = process.argv[2];
	for await (const line of createInterface({ input: process.stdin })) {
		const request = JSON.parse(line);
		try {
			const root = mkdtempSync(join(tmpdir(), "pi-image-generator-"));
			try {
				mkdirSync(join(root, "scripts"));
				if (!request.writeFailure) mkdirSync(join(root, "src"));
				if (request.directoryOutput)
					mkdirSync(join(root, "src/image-models.generated.ts"));
				if (request.preexisting)
					writeFileSync(
						join(root, "src/image-models.generated.ts"),
						"export const IMAGE_MODELS = {prior:true};",
					);
				const script = join(root, "scripts/generate-image-models.ts");
				// Exact source bytes; relative output is isolated, never redirected in source.
				writeFileSync(script, readFileSync(source));
				if (!readFileSync(script).equals(readFileSync(source)))
					throw new Error("temporary source bytes differ");
				writeFileSync(join(root, "package.json"), '{"type":"module"}');
				const preload = join(root, "fetch.mjs");
				const eventPath = join(root, "events.jsonl");
				writeFileSync(
					preload,
					`import {appendFileSync} from "node:fs";
        import {format} from "node:util";
        for (const [method, stream] of [["log", "stdout"], ["error", "stderr"]]) {
          const original = console[method].bind(console);
          console[method] = (...args) => {
            appendFileSync(${JSON.stringify(eventPath)}, JSON.stringify({stream, text: format(...args) + "\\n"}) + "\\n");
            original(...args);
          };
        }
        const nativeFetch = globalThis.fetch;
        globalThis.fetch = async (url, init) => {
        if (url !== "https://openrouter.ai/api/v1/models?output_modalities=image" || (init?.method && init.method !== "GET")) throw new Error("unexpected request");
        const fixture = JSON.parse(process.env.IMAGE_GENERATOR_FIXTURE);
        if (fixture.networkError) throw new Error(fixture.networkError);
        if (fixture.closedLoopback) return nativeFetch(fixture.closedLoopback);
        const body = fixture.bodyError ? new ReadableStream({start(controller) {controller.error(new Error(fixture.bodyError));}}) : fixture.payload;
        return new Response(body, {status: fixture.status ?? 200});
      };`,
				);
				const sourceRoot = join(dirname(source), "../../..");
				const result = spawnSync(
					join(sourceRoot, "node_modules/.bin/tsx"),
					[
						"--tsconfig",
						join(sourceRoot, "tsconfig.json"),
						script,
						...(request.args ?? []),
					],
					{
						encoding: "utf8",
						timeout: 20000,
						env: {
							...process.env,
							NODE_OPTIONS: `${process.env.NODE_OPTIONS ?? ""} --import ${preload}`,
							IMAGE_GENERATOR_FIXTURE: JSON.stringify(request),
						},
					},
				);
				if (result.error) throw result.error;
				let catalog = null;
				if (result.status === 0) {
					const generated = await import(
						pathToFileURL(join(root, "src/image-models.generated.ts")).href
					);
					catalog = JSON.stringify(generated.IMAGE_MODELS);
				}
				process.stdout.write(
					JSON.stringify({
						ok: true,
						status: result.status,
						stdout: result.stdout,
						stderr: result.stderr,
						path: join(root, "src/image-models.generated.ts"),
						catalog,
						events: readFileSync(eventPath, "utf8")
							.trim()
							.split("\n")
							.map((line) => JSON.parse(line)),
					}) + "\n",
				);
			} finally {
				rmSync(root, { recursive: true, force: true });
			}
		} catch (error) {
			process.stdout.write(
				JSON.stringify({
					ok: false,
					name: error.name,
					message: error.message,
				}) + "\n",
			);
		}
	}
}
void main().catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
