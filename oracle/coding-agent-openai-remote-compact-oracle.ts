import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Case = {
	kind: "instructions" | "endpoint" | "headers" | "limit" | "extract" | "payload";
	name: string;
	[key: string]: unknown;
};

void runOracle((api, request) => {
	const buildInstructions = api.buildOpenAICompactInstructions as (base: string, custom?: string) => string;
	const buildEndpoint = api.buildCompactEndpoint as (model: unknown) => string;
	const buildHeaders = api.buildOpenAIRequestHeaders as (
		model: unknown,
		apiKey: string | undefined,
		headers: unknown,
	) => Record<string, string>;
	const limitInput = api.limitOpenAICompactInput as (
		items: unknown[],
		pinned?: Set<unknown>,
		native?: Set<unknown>,
	) => unknown[];
	const extractDetails = api.extractOpenAICompactDetails as (
		model: unknown,
		response: unknown,
		endpoint: string,
	) => unknown;
	const buildPayload = api.buildOpenAICompactPayload as (
		model: unknown,
		messages: unknown[],
		instructions: string,
		previous: unknown[],
		options?: { pinnedMessages?: readonly unknown[] },
	) => unknown;
	return {
		results: requireCases(request).map((value) => {
			const input = value as Case;
			switch (input.kind) {
				case "instructions":
					return {
						name: input.name,
						value: buildInstructions(input.base as string, input.custom as string | undefined),
					};
				case "endpoint":
					return { name: input.name, value: buildEndpoint(input.model) };
				case "headers":
					return {
						name: input.name,
						value: buildHeaders(
							input.model,
							input.apiKey as string | undefined,
							input.headers as Record<string, string | null> | undefined,
						),
					};
				case "limit": {
					const items = input.items as unknown[];
					const pinnedIndices = (input.pinnedIndices as number[] | undefined) ?? [];
					const nativeIndices = (input.nativeIndices as number[] | undefined) ?? [];
					return {
						name: input.name,
						value: limitInput(
							items,
							new Set(pinnedIndices.map((index) => items[index])),
							new Set(nativeIndices.map((index) => items[index])),
						),
					};
				}
				case "extract":
					return {
						name: input.name,
						value: extractDetails(input.model, input.response, input.endpoint as string),
					};
				case "payload": {
					const messages = input.messages as unknown[];
					const pinnedMessageIndices = (input.pinnedMessageIndices as number[] | undefined) ?? [];
					return {
						name: input.name,
						value: buildPayload(
							input.model,
							messages,
							input.instructions as string,
							(input.previous as unknown[]) ?? [],
							{ pinnedMessages: pinnedMessageIndices.map((index) => messages[index]) },
						),
					};
				}
				default:
					throw new Error(`unknown case kind ${input.kind}`);
			}
		}),
	};
}).catch((error: unknown) => {
	process.stderr.write(`openai-remote-compact oracle startup failed: ${String(error)}\n`);
	process.exitCode = 1;
});
