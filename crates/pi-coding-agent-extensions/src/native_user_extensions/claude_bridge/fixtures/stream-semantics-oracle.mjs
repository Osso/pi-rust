// Offline oracle: copied convert.ts, extract-tool-results.ts and parsePartialJson.ts.
// The last file contains the unchanged helper extracted from source index.ts.
// Run with tsx and the source extension's node_modules in a scratch ESM directory.
import { pathToFileURL } from "node:url";
const { convertPiMessages } = await import(pathToFileURL(process.argv[2]));
const { toolResultToMcpContent } = await import(pathToFileURL(process.argv[3]));
const { parsePartialJson } = await import(pathToFileURL(process.argv[4]));
const inputs = [undefined, "", [], [{type: "text", text: ""}], [{type: "thinking", thinking: "hidden"}], [
    {type: "text", text: ""}, {type: "text", text: "kept \ud800"},
    {type: "image", data: "AQ==", mimeType: "image/png"}, {type: "image", data: "", mimeType: "image/png"},
]];
const mcp = inputs.map(input => ({input: input ?? null, output: toolResultToMcpContent(input)}));
const messages = inputs.filter(Array.isArray).slice(0, 2).map((content, index) => ({
    role: "toolResult", toolCallId: `tool-${index}`, toolName: "read", content, isError: index === 1, timestamp: 1,
}));
const partial = [];
let accumulated = "";
let argumentsValue = {command: "seed"};
for (const delta of ['{"command":"new ', '\\ud800","nested":{"key":1}}', ',']) {
    accumulated += delta;
    argumentsValue = parsePartialJson(accumulated, argumentsValue);
    partial.push({delta, accumulated, arguments: argumentsValue});
}
console.log(JSON.stringify({source: "38d84cb549cce66dd76d45a5020d132f74780393", mcp, messages,
    converted: convertPiMessages(messages, new Map([["read", "mcp__custom-tools__read"]])).anthropicMessages,
    partial, finalArguments: parsePartialJson(accumulated, argumentsValue)}, null, 2));
