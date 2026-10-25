import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Component = { render(width: number): string[] };
type Options = { sessionId: string; expanded: boolean; requestRender(): boolean; registerCleanup(cleanup: () => void): void };
type Renderer = (entry: unknown, options: Options, theme: unknown) => Component | undefined;
type Binding = { callback: () => boolean; token: symbol };

runOracle(async (api, request) => {
	return requireCases(request).map(value => {
		const steps = value as { id: string; session: string; message: string; displayId?: string; cleanup?: boolean; width?: number }[];
		const bindings = new Map<string, Binding>();
		const refresher = { bind(session: string, reviewAt: string, callback: () => boolean) {
			const key = `${session}:${reviewAt}`;
			const token = Symbol();
			bindings.set(key, { callback, token });
			return () => { if (bindings.get(key)?.token === token) bindings.delete(key); };
		} };
		const render = (api.createSupervisorStatusEntryRenderer as (refresher: unknown) => Renderer)(refresher);
		const theme = { fg: (_color: string, text: string) => text, bg: (_color: string, text: string) => text, bold: (text: string) => text };
		const components: Component[] = [];
		const cleanups: (() => void)[] = [];
		return steps.map(step => {
			if (step.cleanup) for (const cleanup of cleanups.splice(0)) cleanup();
			const component = render({ id: step.id, data: { message: step.message, displayId: step.displayId } }, {
				sessionId: step.session, expanded: false, requestRender: () => true, registerCleanup: cleanup => cleanups.push(cleanup),
			}, theme);
			if (component) components.push(component);
			return { returned: !!component, lines: components.map(item => item.render(step.width ?? 32)) };
		});
	});
}).catch(error => { console.error(error); process.exitCode = 1; });
