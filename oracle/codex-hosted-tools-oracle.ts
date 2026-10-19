// Differential oracle for the bundled `codex-web-search` and
// `codex-image-generation` extensions.

import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

type AnyRecord = Record<string, any>;

const sourcePath = process.argv[2] ?? "";
const extensionsDir = dirname(dirname(dirname(sourcePath)));

function model(input: AnyRecord): AnyRecord {
	return {
		id: "test-model",
		name: "Test Model",
		api: input.api,
		provider: input.provider,
		baseUrl: "https://example.test",
		reasoning: false,
		input: ["text", "image"],
		cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
		contextWindow: 1000,
		maxTokens: 1000,
	};
}

function metadata(tool: AnyRecord): AnyRecord {
	return {
		name: tool.name,
		label: tool.label,
		description: tool.description,
		promptSnippet: tool.promptSnippet ?? null,
		promptGuidelines: tool.promptGuidelines ?? null,
		approvalRequired: tool.approvalRequired ?? null,
		parameters: tool.parameters,
	};
}

function normalizePath(text: string): string {
	return text.replace(/image-gen-[a-f0-9]{16}\.png/g, "image-gen-<hex>.png");
}

void runOracle(async (api, request) => {
	const cases = (request.cases as AnyRecord[]) ?? [];
	const imageModule = (await import(
		pathToFileURL(join(extensionsDir, "codex-image-generation/src/index.ts")).href
	)) as AnyRecord;
	const results: AnyRecord[] = [];
	for (const input of cases) {
		const output: AnyRecord = { name: input.name };
		if (input.kind === "web_payload") {
			output.value = api.addWebSearchToolToPayload(input.payload) ?? null;
		} else if (input.kind === "image_payload") {
			output.value = imageModule.addImageGenerationToolToPayload(input.payload) ?? null;
		} else if (input.kind === "web_model") {
			output.value = api.isOpenAIHostedWebSearchModel(model(input.model));
		} else if (input.kind === "image_model") {
			output.value = imageModule.isOpenAIHostedImageGenerationModel(model(input.model));
		} else if (input.kind === "web_metadata") {
			output.metadata = metadata(api.createWebSearchToolDefinition());
		} else if (input.kind === "image_metadata") {
			output.metadata = metadata(imageModule.createImageGenerationToolDefinition());
		} else if (input.kind === "web_execute") {
			let captured: AnyRecord | undefined;
			const tool = api.createWebSearchToolDefinition({
				runSearch: async (searchRequest: AnyRecord) => {
					captured = {
						query: searchRequest.query,
						apiKey: searchRequest.apiKey ?? null,
						hasSignal: searchRequest.signal !== undefined,
					};
					if (input.runError) throw new Error(input.runError);
					return input.runResult ?? "Search result text";
				},
				timeoutMs: input.timeoutMs,
			});
			const ctx = {
				model: model(input.model),
				modelRegistry: {
					getApiKeyAndHeaders: async () => ({ ok: true, apiKey: "test-key" }),
				},
			};
			output.error = null;
			try {
				const result = await tool.execute(
					"call-1",
					{ query: input.query },
					undefined,
					undefined,
					ctx,
				);
				output.text = result.content.map((item: AnyRecord) => item.text ?? null);
				output.captured = captured ?? null;
			} catch (error) {
				output.error = error instanceof Error ? error.message : String(error);
				output.captured = captured ?? null;
			}
		} else if (input.kind === "image_execute") {
			let captured: AnyRecord | undefined;
			const tool = imageModule.createImageGenerationToolDefinition({
				runGeneration: async (generationRequest: AnyRecord) => {
					captured = {
						prompt: generationRequest.prompt,
						apiKey: generationRequest.apiKey ?? null,
						hasSignal: generationRequest.signal !== undefined,
					};
					if (input.runError) throw new Error(input.runError);
					return { type: "image", data: "aW1hZ2U=", mimeType: "image/png" };
				},
				timeoutMs: input.timeoutMs,
			});
			const ctx = {
				model: model(input.model),
				modelRegistry: {
					getApiKeyAndHeaders: async () => ({ ok: true, apiKey: "test-key" }),
				},
				cwd: input.cwd,
			};
			output.error = null;
			try {
				const result = await tool.execute(
					"call-1",
					{ prompt: input.prompt },
					undefined,
					undefined,
					ctx,
				);
				output.content = result.content.map((item: AnyRecord) =>
					item.type === "text"
						? { type: "text", text: normalizePath(item.text) }
						: item,
				);
				output.captured = captured ?? null;
			} catch (error) {
				output.error = error instanceof Error ? error.message : String(error);
				output.captured = captured ?? null;
			}
		}
		results.push(output);
	}
	return { results };
}).catch((error: unknown) => {
	process.stderr.write(`hosted tools oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
