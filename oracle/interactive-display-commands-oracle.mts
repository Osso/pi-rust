import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const source = process.argv[2];
if (!source) throw new Error("coding-agent source root required");
const load = (path: string) => import(pathToFileURL(`${source}/${path}`).href);
const { AgentSession } = await load("core/agent-session.ts");
const { SessionManager } = await load("core/session-manager.ts");
const { InteractiveMode } = await load("modes/interactive/interactive-mode.ts");
const { Container } = await load("../../tui/src/index.ts");
const { Theme, setThemeInstance } = await load(
	"modes/interactive/theme/theme.ts",
);
const units = (text: string) =>
	Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
interface Step {
	command: "name" | "unname" | "tools" | "extensions";
	args: string;
	active: string[];
}
interface Request {
	sessionPath: string;
	controlDb: string;
	colors: string[];
	tools: unknown[];
	extensions: {
		path: string;
		sourceInfo: unknown;
		commands: number;
		tools: number;
		handlers: number;
	}[];
	steps: Step[];
}
function sizeMap(size: number) {
	return new Map(Array.from({ length: size }, (_, i) => [`${i}`, null]));
}
async function capture(request: Request) {
	setThemeInstance(
		new Theme(
			Object.fromEntries(request.colors.map((name) => [name, 0])),
			{},
			"256color",
		),
	);
	const manager = SessionManager.open(request.sessionPath);
	manager.setMetadataControlDbPath(request.controlDb);
	const session = Object.create(AgentSession.prototype);
	session.sessionManager = manager;
	const events: (string | null)[] = [];
	session._emit = (event: { name?: string }) => {
		events.push(event.name ?? null);
	};
	session._extensionRunner = { emit: async () => undefined };
	session.getAllTools = () => request.tools;
	session._resourceLoader = {
		getExtensions: () => ({
			extensions: request.extensions.map((extension) => ({
				...extension,
				commands: sizeMap(extension.commands),
				tools: sizeMap(extension.tools),
				handlers: sizeMap(extension.handlers),
			})),
		}),
	};
	const titles: number[][] = [];
	const mode = Object.assign(Object.create(InteractiveMode.prototype), {
		runtimeHost: { session },
		chatContainer: new Container(),
		ui: {
			requestRender() {},
			terminal: {
				setTitle(title: string) {
					titles.push(units(title));
				},
			},
		},
	});
	const snapshots = [];
	for (const step of request.steps) {
		session.getActiveToolNames = () => step.active;
		if (step.command === "name") mode.handleNameCommand(`/name ${step.args}`);
		if (step.command === "unname") mode.handleUnnameCommand();
		if (step.command === "tools") mode.handleToolsCommand();
		if (step.command === "extensions") mode.handleExtensionsCommand();
		const reopened = SessionManager.open(request.sessionPath);
		reopened.setMetadataControlDbPath(request.controlDb);
		snapshots.push({
			frames: [25, 120].map((width) =>
				mode.chatContainer.render(width).map(units),
			),
			name: manager.getSessionName() ?? null,
			persistedName: reopened.getSessionName() ?? null,
			events: [...events],
			titles: [...titles],
		});
	}
	return snapshots;
}
for await (const line of createInterface({ input: process.stdin })) {
	try {
		console.log(
			JSON.stringify({
				ok: true,
				snapshots: await capture(JSON.parse(line) as Request),
			}),
		);
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
