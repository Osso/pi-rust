// Offline source oracle. Arguments: copied snapshot convert.ts, cc-session-io, input.json.
// Originals remain read-only. No account or network is used.
import { readFileSync } from "node:fs";
import crypto from "node:crypto";
import { syncBuiltinESMExports } from "node:module";
import { pathToFileURL } from "node:url";

let sequence = 0;
crypto.randomUUID = () => `${(++sequence).toString(16).padStart(8, "0")}-1111-4111-8111-111111111111`;
syncBuiltinESMExports();
const NativeDate = Date;
globalThis.Date = class extends NativeDate {
    constructor(...args) { super(...(args.length ? args : [1792929600000])); }
    static now() { return 1792929600000; }
};
Math.random = () => 0;

const convert = await import(pathToFileURL(process.argv[2]));
const sessionIo = await import(pathToFileURL(process.argv[3]));
const input = JSON.parse(readFileSync(process.argv[4], "utf8"));
const toolMap = new Map(["read", "bash"].map(name => [name, `mcp__custom-tools__${name}`]));
const provider = convert.convertPiMessages(input, toolMap).anthropicMessages;
const ask = convert.convertPiMessages(input).anthropicMessages;
const session = new sessionIo.Session({ sessionId: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", projectPath: "/fixture/project", jsonlPath: "/unused", cwd: "/fixture/project", model: "claude-opus-5-5[1m]" });
session.importMessages(provider);
console.log(JSON.stringify({ source: "38d84cb549cce66dd76d45a5020d132f74780393", input, provider, ask, repaired: sessionIo.repairToolPairing(provider), records: session.records, jsonl: sessionIo.serializeJsonl(session.records) }));
