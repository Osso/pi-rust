import { pathToFileURL } from "node:url";

process.once("uncaughtException", (error: Error) => {
	const record: Record<string, unknown> = {
		name: error.name,
		message: error.message,
	};
	for (const field of ["code", "errno", "syscall"]) {
		if (field in error) record[field] = Reflect.get(error, field);
	}
	process.stdout.write(JSON.stringify(record) + "\n", () => process.exit(1));
});
const api = (await import(pathToFileURL(process.argv[2]).href)) as {
	RpcClient: new (options: {
		cliPath: string;
	}) => { start(): Promise<void>; getState(): Promise<unknown> };
};
const script = process.env.PI_RPC_MIRROR_SCRIPT;
if (!script) throw new Error("missing private mirror script");
const client = new api.RpcClient({ cliPath: script });
await client.start();
await client.getState();
