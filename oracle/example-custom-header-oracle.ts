import { registerHooks } from "node:module";
import { runOracle } from "./coding-agent-utils-oracle-support.ts";

// Only the example's public package import is controlled; its module bytes stay pinned.
const version = process.env.CUSTOM_HEADER_ORACLE_VERSION;
if (version === undefined)
	throw new Error("custom-header version fixture required");
const fixture = `data:text/javascript,${encodeURIComponent(`export const VERSION = ${JSON.stringify(version)};`)}`;
registerHooks({
	resolve(specifier, context, nextResolve) {
		if (specifier === "@earendil-works/pi-coding-agent") {
			return { url: fixture, shortCircuit: true };
		}
		return nextResolve(specifier, context);
	},
});

type Component = { render(width: number): string[]; invalidate(): void };
type Context = {
	mode: string;
	ui: {
		setHeader(factory?: (_tui: unknown, theme: unknown) => Component): void;
		notify(message: string, level: string): void;
	};
};
type Command = {
	description: string;
	handler(args: string, ctx: Context): Promise<void>;
};

runOracle(async (api, request) => {
	const calls: unknown[] = [];
	const registrations: unknown[] = [];
	let start: ((_event: unknown, ctx: Context) => Promise<void>) | undefined;
	let command: Command | undefined;
	const theme = {
		fg(role: string, text: string) {
			const colors = request.colors as Record<string, number[]>;
			return `\x1b[38;2;${colors[role].join(";")}m${text}\x1b[39m`;
		},
	};
	(api.default as (pi: unknown) => void)({
		on(name: string, handler: typeof start) {
			registrations.push(["on", name]);
			start = handler;
		},
		registerCommand(name: string, options: Command) {
			registrations.push(["command", name, options.description]);
			command = options;
		},
	});
	const ctx: Context = {
		mode: request.mode as string,
		ui: {
			setHeader(factory) {
				calls.push(factory ? ["setHeader"] : ["clearHeader"]);
				if (!factory) return;
				const component = factory({}, theme);
				calls.push(["factory"]);
				for (const width of request.widths as number[])
					calls.push(["render", width, component.render(width)]);
				component.invalidate();
				calls.push(["invalidate"]);
				calls.push(["render", 1, component.render(1)]);
			},
			notify(message, level) {
				calls.push(["notify", message, level]);
			},
		},
	};
	if (!start || !command) throw new Error("missing custom-header registration");
	await start({}, ctx);
	await command.handler("ignored args", ctx);
	return { registrations, calls };
}).catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
