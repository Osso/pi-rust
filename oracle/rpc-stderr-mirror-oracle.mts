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
	}) => {
		start(): Promise<void>;
		stop(): Promise<void>;
		getState(): Promise<unknown>;
		send(command: { type: "get_state" }): Promise<unknown>;
		getStderr(): string;
	};
};
const script = process.env.PI_RPC_MIRROR_SCRIPT;
if (!script) throw new Error("missing private mirror script");
const client = new api.RpcClient({ cliPath: script });
await client.start();
if (process.env.PI_RPC_MIRROR_DISCARD === "1") {
	const response = await client.send({ type: "get_state" });
	const deadline = Date.now() + 2000;
	while (!client.getStderr()) {
		if (Date.now() > deadline) throw new Error("missing private stderr data");
		await new Promise((resolve) => setTimeout(resolve, 1));
	}
	const stderr = client.getStderr();
	await client.stop();
	process.stdout.write(JSON.stringify({ response, stderr }) + "\n", () =>
		process.exit(0),
	);
} else {
	await client.getState();
}
