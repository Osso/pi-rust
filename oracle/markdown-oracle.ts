import { StringDecoder } from "node:string_decoder";
import { pathToFileURL } from "node:url";
import * as path from "node:path";

type RequestId = string | number | null;

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

const STRICT_STRIKETHROUGH_REGEX = /^(~~)(?=[^\s~])((?:\\.|[^\\])*?(?:\\.|[^\s~\\]))\1(?=[^~]|$)/;

function trimPartialClosingFences(tokens: readonly any[]): void {
	const token = tokens[tokens.length - 1];
	if (token?.type === "list") {
		trimPartialClosingFences(token.items[token.items.length - 1]?.tokens ?? []);
		return;
	}
	if (token?.type === "blockquote") {
		trimPartialClosingFences(token.tokens ?? []);
		return;
	}
	if (token?.type !== "code") return;
	const marker = /^(`{3,}|~{3,})/.exec(token.raw)?.[1];
	const lastLine = token.raw.split("\n").pop();
	if (!marker || !lastLine || lastLine.length >= marker.length || lastLine !== marker[0]?.repeat(lastLine.length)) {
		return;
	}
	token.text = token.text.slice(0, -lastLine.length).replace(/\n$/, "");
}

let parser: any;

async function getParser(sourceDirectory: string): Promise<any> {
	if (parser) return parser;
	const markedModule = (await import(
		pathToFileURL(path.join(sourceDirectory, "../../../node_modules/marked/lib/marked.esm.js")).href
	)) as any;
	const { Marked, Tokenizer } = markedModule;
	class StrictStrikethroughTokenizer extends Tokenizer {
		override del(src: string) {
			const match = STRICT_STRIKETHROUGH_REGEX.exec(src);
			if (!match) return undefined;
			const text = match[2];
			return {
				type: "del",
				raw: match[0],
				text,
				tokens: this.lexer.inlineTokens(text),
			};
		}
	}
	const instance = new Marked();
	instance.setOptions({ tokenizer: new StrictStrikethroughTokenizer() });
	parser = instance;
	return parser;
}

async function loadApi(sourceDirectory: string, moduleName: string): Promise<unknown> {
	return import(pathToFileURL(path.join(sourceDirectory, moduleName)).href);
}

async function execute(request: Request, sourceDirectory: string): Promise<unknown> {
	switch (request.operation) {
		case "markdown_lex": {
			const instance = await getParser(sourceDirectory);
			const cases = requireArray<{ name: string; markdown: string }>(request, "cases");
			return {
				results: cases.map((testCase) => {
					const normalized = testCase.markdown.replace(/\t/g, "   ");
					const tokens = instance.lexer(normalized);
					trimPartialClosingFences(tokens);
					return { name: testCase.name, tokens };
				}),
			};
		}
		case "markdown_render": {
			const markdownModule = (await loadApi(sourceDirectory, "components/markdown.ts")) as any;
			const themeModule = (await loadApi(path.join(sourceDirectory, ".."), "test/test-themes.ts")) as any;
			const terminalImage = (await loadApi(sourceDirectory, "terminal-image.ts")) as any;
			const chalkModule = (await import(
				pathToFileURL(path.join(sourceDirectory, "../../../node_modules/chalk/source/index.js")).href
			)) as any;
			const chalk = new chalkModule.Chalk({ level: 3 });
			const cases = requireArray<any>(request, "cases");
			const results = cases.map((testCase) => {
				terminalImage.setCapabilities({
					images: null,
					trueColor: false,
					hyperlinks: testCase.hyperlinks,
				});
				let defaultTextStyle: Record<string, unknown> | undefined;
				if (testCase.defaultTextStyle) {
					const style = testCase.defaultTextStyle;
					const chalkFor = (name: string | undefined) => {
						if (!name) return undefined;
						const builder = chalk[name];
						if (!builder) throw new Error(`unknown chalk style ${name}`);
						return (text: string) => builder(text);
					};
					defaultTextStyle = {
						color: chalkFor(style.color),
						bgColor: chalkFor(style.bgColor),
						bold: style.bold,
						italic: style.italic,
						strikethrough: style.strikethrough,
						underline: style.underline,
					};
				}
				const markdown = new markdownModule.Markdown(
					testCase.markdown,
					testCase.paddingX,
					testCase.paddingY,
					themeModule.defaultMarkdownTheme,
					defaultTextStyle,
					testCase.options,
				);
				const lines = markdown.render(testCase.width);
				terminalImage.resetCapabilitiesCache();
				return { name: testCase.name, lines };
			});
			return { results };
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

function readLines(stream: NodeJS.ReadableStream, onLine: (line: string) => void): void {
	const decoder = new StringDecoder("utf8");
	let buffer = "";
	stream.on("data", (chunk: Buffer) => {
		buffer += decoder.write(chunk);
		let newline = buffer.indexOf("\n");
		while (newline !== -1) {
			const line = buffer.slice(0, newline);
			buffer = buffer.slice(newline + 1);
			if (line.length > 0) onLine(line);
			newline = buffer.indexOf("\n");
		}
	});
	stream.on("end", () => {
		buffer += decoder.end();
		if (buffer.length > 0) onLine(buffer);
	});
}

const sourceDirectory = process.argv[2];
if (!sourceDirectory) throw new Error("source directory argument required");
readLines(process.stdin, (line) => {
	void executeRequestLine(line, sourceDirectory);
});
