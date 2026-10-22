import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";

type RequestId = string | number | null;

type AutocompleteItem = { value: string; label: string; description?: string };

type SlashCommand = {
	name: string;
	description?: string;
	argumentHint?: string;
	getArgumentCompletions?: (prefix: string) => AutocompleteItem[] | null;
};

type ProviderApi = {
	CombinedAutocompleteProvider: new (
		commands: Array<SlashCommand | AutocompleteItem>,
		basePath: string,
		fdPath?: string | null,
	) => {
		getSuggestions(
			lines: string[],
			cursorLine: number,
			cursorCol: number,
			options: { signal: AbortSignal; force?: boolean },
		): Promise<{ items: AutocompleteItem[]; prefix: string } | null>;
		applyCompletion(
			lines: string[],
			cursorLine: number,
			cursorCol: number,
			item: AutocompleteItem,
			prefix: string,
		): { lines: string[]; cursorLine: number; cursorCol: number };
	};
};

type Request = {
	id: RequestId;
	operation: string;
	[key: string]: unknown;
};

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) return { name: error.name, message: error.message };
	return { name: "UnknownError", message: String(error) };
}

function requireArray<T>(request: Request, key: string): T[] {
	const value = request[key];
	if (!Array.isArray(value)) throw new Error(`${request.operation} requires array field ${key}`);
	return value as T[];
}

function requireString(request: Request, key: string): string {
	const value = request[key];
	if (typeof value !== "string") throw new Error(`${request.operation} requires string field ${key}`);
	return value;
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<unknown> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

type CommandRequest = {
	name?: string | null;
	value?: string | null;
	label?: string | null;
	description?: string | null;
	argumentHint?: string | null;
	argumentCompletions?: AutocompleteItem[] | null;
};

function buildCommands(requests: CommandRequest[]): Array<SlashCommand | AutocompleteItem> {
	return requests.map((command) => {
		if (command.value !== null && command.value !== undefined) {
			const item: AutocompleteItem = {
				value: command.value,
				label: command.label ?? command.value,
			};
			if (command.description !== null && command.description !== undefined) {
				item.description = command.description;
			}
			return item;
		}
		const result: SlashCommand = { name: command.name ?? "" };
		if (command.description !== null && command.description !== undefined) {
			result.description = command.description;
		}
		if (command.argumentHint !== null && command.argumentHint !== undefined) {
			result.argumentHint = command.argumentHint;
		}
		if (command.argumentCompletions !== null && command.argumentCompletions !== undefined) {
			const completions = command.argumentCompletions;
			result.getArgumentCompletions = () => completions;
		}
		return result;
	});
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "autocomplete": {
			const api = (await loadApi(sourceDirectory, "autocomplete.ts")) as ProviderApi;
			const basePath = requireString(request, "basePath");
			const fdPath = typeof request["fdPath"] === "string" ? (request["fdPath"] as string) : null;
			const commands = buildCommands(requireArray<CommandRequest>(request, "commands"));
			const provider = new api.CombinedAutocompleteProvider(commands, basePath, fdPath);
			const cases = requireArray<{
				lines: string[];
				cursorLine: number;
				cursorCol: number;
				force: boolean;
			}>(request, "cases");

			const results = [];
			for (const testCase of cases) {
				const cursorCol = testCase.cursorCol;
				const result = await provider.getSuggestions(testCase.lines, testCase.cursorLine, cursorCol, {
					signal: new AbortController().signal,
					force: testCase.force,
				});
				results.push(
					result === null
						? null
						: {
								prefix: result.prefix,
								items: result.items.map((item) => ({
									value: item.value,
									label: item.label,
									description: item.description ?? null,
								})),
							},
				);
			}
			return { results };
		}
		case "autocomplete_apply": {
			const api = (await loadApi(sourceDirectory, "autocomplete.ts")) as ProviderApi;
			const basePath = requireString(request, "basePath");
			const provider = new api.CombinedAutocompleteProvider([], basePath, null);
			const cases = requireArray<{
				lines: string[];
				cursorLine: number;
				cursorCol: number;
				item: AutocompleteItem;
				prefix: string;
			}>(request, "cases");

			return {
				results: cases.map((testCase) => {
						const cursorCol = testCase.cursorCol;
					const applied = provider.applyCompletion(
						testCase.lines,
						testCase.cursorLine,
						cursorCol,
						testCase.item,
						testCase.prefix,
					);
					return {
						lines: applied.lines,
						cursorLine: applied.cursorLine,
						cursorCol: applied.cursorCol,
					};
				}),
			};
		}
		default:
			throw new Error(`unsupported operation: ${request.operation}`);
	}
}

async function executeRequestLine(line: string, sourceDirectory: string): Promise<void> {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const output = await execute(request, sourceDirectory);
		const response: Response = { id, ok: true, output };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(sourceDirectory: string): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : decoder.write(chunk);
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			const line = buffer.slice(0, lineFeed);
			buffer = buffer.slice(lineFeed + 1);
			void executeRequestLine(line, sourceDirectory);
		}
	});
}

function readSourceDirectory(): string {
	const sourceDirectory = process.argv[2];
	if (sourceDirectory === undefined) {
		throw new Error("autocomplete oracle requires a source directory path");
	}
	return sourceDirectory;
}

serveRequests(readSourceDirectory());
