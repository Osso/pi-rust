import { appendFileSync } from "node:fs";

const source = process.env.PI_RPC_FROZEN_MODULE;
const output = process.env.PI_RPC_WIRE_LOG;
if (source === undefined || output === undefined)
	throw new Error("source and output required");
const write = process.stdout.write.bind(process.stdout);
process.stdout.write = ((...args: Parameters<typeof process.stdout.write>) => {
	appendFileSync(output, args[0]);
	return write(...args);
}) as typeof process.stdout.write;
process.argv[2] = source;
await import(new URL("./modes-harness.ts", import.meta.url).href);
