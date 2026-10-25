import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import {
	requireCases,
	runOracle,
} from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Options = {
	sessionId: string;
	expanded: boolean;
	requestRender(): boolean;
	registerCleanup(cleanup: () => void): void;
};
type Renderer = (
	entry: unknown,
	options: Options,
	theme: unknown,
) => Component | undefined;
type Binding = { callback: () => boolean; token: symbol };

runOracle(async (api, request) => {
	if (request.operation === "restore") {
		const countdown = await import(
			pathToFileURL(join(dirname(process.argv[2]), "wait-countdown.ts")).href
		);
		const oldNow = Date.now;
		const oldSet = globalThis.setTimeout;
		const oldClear = globalThis.clearTimeout;
		let now = oldNow();
		let next = 0;
		const timers = new Map<number, { at: number; callback: () => void }>();
		Date.now = () => now;
		globalThis.setTimeout = ((callback: () => void, delay: number) => {
			timers.set(++next, { at: now + delay, callback });
			return next;
		}) as unknown as typeof setTimeout;
		globalThis.clearTimeout = ((id: number) => {
			timers.delete(id);
		}) as unknown as typeof clearTimeout;
		try {
			return requireCases(request).map((value) => {
				const input = value as { entries: unknown[]; reviewAt: string };
				const refresher = countdown.createWaitCountdownRefresher();
				let paints = 0;
				const controller = (
					api.createSupervisorStatusController as (
						pi: unknown,
						refresher: unknown,
					) => { restore(ctx: unknown): void }
				)({}, refresher);
				controller.restore({
					sessionManager: {
						getSessionId: () => "session",
						getEntries: () => input.entries,
					},
				});
				refresher.bind("session", input.reviewAt, () => {
					paints++;
					return true;
				});
				const first = [...timers.entries()][0];
				if (first) {
					timers.delete(first[0]);
					now = first[1].at;
					first[1].callback();
				}
				refresher.clearAll();
				return paints;
			});
		} finally {
			Date.now = oldNow;
			globalThis.setTimeout = oldSet;
			globalThis.clearTimeout = oldClear;
		}
	}
	return requireCases(request).map((value) => {
		const steps = value as {
			id: string;
			session: string;
			message: string;
			displayId?: string;
			cleanup?: boolean;
			width?: number;
		}[];
		const bindings = new Map<string, Binding>();
		const refresher = {
			bind(session: string, reviewAt: string, callback: () => boolean) {
				const key = `${session}:${reviewAt}`;
				const token = Symbol();
				bindings.set(key, { callback, token });
				return () => {
					if (bindings.get(key)?.token === token) bindings.delete(key);
				};
			},
		};
		const render = (
			api.createSupervisorStatusEntryRenderer as (
				refresher: unknown,
			) => Renderer
		)(refresher);
		const theme = {
			fg: (_color: string, text: string) => text,
			bg: (_color: string, text: string) => text,
			bold: (text: string) => text,
		};
		const components: Component[] = [];
		const cleanups: (() => void)[] = [];
		return steps.map((step) => {
			if (step.cleanup) for (const cleanup of cleanups.splice(0)) cleanup();
			const component = render(
				{
					id: step.id,
					data: { message: step.message, displayId: step.displayId },
				},
				{
					sessionId: step.session,
					expanded: false,
					requestRender: () => true,
					registerCleanup: (cleanup) => cleanups.push(cleanup),
				},
				theme,
			);
			if (component) components.push(component);
			return {
				returned: !!component,
				lines: components.map((item) => item.render(step.width ?? 32)),
			};
		});
	});
}).catch((error) => {
	console.error(error);
	process.exitCode = 1;
});
