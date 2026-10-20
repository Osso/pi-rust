import { pathToFileURL } from "node:url";

// Import only the pinned example; never load bundled extensions or session hooks.
const modulePromise = import(pathToFileURL(process.argv[2]).href);

async function run(request: any) {
	const { default: register } = await modulePromise;
	const registrations: any[] = [];
	const actions: any[] = [];
	let commands: any[] = [];
	register({
		registerCommand(name: string, command: any) {
			registrations.push({ name, ...command });
		},
		getCommands() {
			actions.push({ type: "getCommands" });
			return commands;
		},
	});
	const metadata = registrations.map(({ name, description, getArgumentCompletions, handler }) => ({
		name, description,
		hasArgumentCompletions: typeof getArgumentCompletions === "function",
		hasHandler: typeof handler === "function",
	}));
	const command = registrations[0];
	const completions = request.prefixes.map((prefix: string) => command.getArgumentCompletions(prefix));
	const invocations: any[] = [];
	for (const invocation of request.invocations) {
		commands = invocation.commands;
		actions.length = 0;
		await command.handler(invocation.args, { ui: {
			select: async (title: string, items: string[]) => {
				actions.push({ type: "select", title, items });
				return invocation.selected ?? undefined;
			},
			confirm: async (title: string, message: string) => {
				actions.push({ type: "confirm", title, message });
				return invocation.confirm;
			},
			notify: (message: string, level: string) => actions.push({ type: "notify", message, level }),
		} });
		invocations.push([...actions]);
	}
	return { metadata, completions, invocations };
}

async function main() {
	// JSON permits literal U+2028/U+2029; readline treats them as line ends.
	// Frame only on the protocol's ASCII LF, preserving every scalar unchanged.
	process.stdin.setEncoding("utf8");
	let pending = "";
	for await (const chunk of process.stdin) {
		pending += chunk;
		let end: number;
		while ((end = pending.indexOf("\n")) !== -1) {
			const line = pending.slice(0, end);
			pending = pending.slice(end + 1);
			console.log(JSON.stringify(await run(JSON.parse(line))));
		}
	}
	if (pending.length) throw new Error("Unterminated oracle request");
}
main().catch((error) => { console.error(error); process.exit(1); });
