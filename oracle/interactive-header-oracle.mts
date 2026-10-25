import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { runInNewContext } from "node:vm";

const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const source = `${root}/modes/interactive/interactive-mode.ts`;
const packageRoot = root.slice(0, root.indexOf("/packages/"));
const ts: typeof import("@pi-oracle-source/../node_modules/typescript/lib/typescript.js") =
	createRequire(resolve(packageRoot, "package.json"))("typescript");
const ast = ts.createSourceFile(
	source,
	readFileSync(source, "utf8"),
	ts.ScriptTarget.Latest,
	true,
);
const modeClass = ast.statements
	.filter(ts.isClassDeclaration)
	.find((node) => node.name?.text === "InteractiveMode");
const init = modeClass?.members.find(
	(node) => node.name?.getText(ast) === "init",
);
if (!init) throw new Error("frozen init missing");
const blocks: import("@pi-oracle-source/../node_modules/typescript/lib/typescript.js").IfStatement[] =
	[];
function visit(
	node: import("@pi-oracle-source/../node_modules/typescript/lib/typescript.js").Node,
) {
	if (
		ts.isIfStatement(node) &&
		node.expression.getText(ast) ===
			"this.options.verbose || !this.settingsManager.getQuietStartup()"
	)
		blocks.push(node);
	ts.forEachChild(node, visit);
}
visit(init);
const declarations = ast.statements.filter(
	(node) =>
		(ts.isFunctionDeclaration(node) && node.name?.text === "isExpandable") ||
		(ts.isClassDeclaration(node) && node.name?.text === "ExpandableText"),
);
if (blocks.length !== 1 || declarations.length !== 2)
	throw new Error("frozen header declarations/block missing");
const { InteractiveMode } = await import(pathToFileURL(source).href);
const tui = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const themes = await import(
	pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href
);
const hints = await import(
	pathToFileURL(`${root}/modes/interactive/components/keybinding-hints.ts`).href
);
const config = await import(pathToFileURL(`${root}/config.ts`).href);
const { KeybindingsManager } = await import(
	pathToFileURL(`${root}/core/keybindings.ts`).href
);
const frozenDeclarations = declarations.map((node) => node.getText(ast)).join("\n");
const javascript = ts.transpileModule(
	`${frozenDeclarations}
	globalThis.buildHeader = function() { ${blocks[0].getText(ast)} };
	globalThis.makeExpandable = (name) => new ExpandableText(
		() => name + "-collapsed", () => name + "-expanded", false, 0, 0,
	);`,
	{ compilerOptions: { target: ts.ScriptTarget.ES2022 } },
).outputText;
const units = (text: string) =>
	Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		themes.setThemeInstance(
			new themes.Theme(
				Object.fromEntries(request.colors.map((name: string) => [name, 0])),
				{},
				"256color",
			),
		);
		tui.setKeybindings(new KeybindingsManager(request.bindings));
		const context = {
			Text: tui.Text,
			Spacer: tui.Spacer,
			theme: themes.theme,
			APP_NAME: config.APP_NAME,
			...hints,
		};
		runInNewContext(javascript, context);
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), {
			options: { verbose: request.verbose },
			toolOutputExpanded: request.expanded,
			runtimeHost: {
				session: { settingsManager: { getQuietStartup: () => request.quiet } },
			},
			version: request.version,
			headerContainer: new tui.Container(),
			loadedResourcesContainer: new tui.Container(),
			chatContainer: new tui.Container(),
			ui: { requestRender() {} },
		});
		context.buildHeader.call(receiver);
		receiver.loadedResourcesContainer.addChild(
			context.makeExpandable("resource"),
		);
		receiver.chatContainer.addChild(context.makeExpandable("chat"));
		receiver.chatContainer.addChild(new tui.Text("plain child", 0, 0));
		const capture = () => ({
			header: receiver.headerContainer.render(request.width).map(units),
			resources: receiver.loadedResourcesContainer
				.render(request.width)
				.map(units),
			chat: receiver.chatContainer.render(request.width).map(units),
		});
		const frames: ReturnType<typeof capture>[] = [];
		const captureExpansion = () => {
			frames.push(capture());
			for (const expanded of [true, false]) {
				receiver.setToolsExpanded(expanded);
				frames.push(capture());
			}
		};
		captureExpansion();
		for (const expandable of [true, false]) {
			receiver.setExtensionHeader(() => {
				const component = new tui.Text("custom", 0, 0);
				if (expandable)
					component.setExpanded = (value: boolean) =>
						component.setText(value ? "custom-expanded" : "custom-collapsed");
				return component;
			});
			captureExpansion();
		}
		receiver.setExtensionHeader(undefined);
		captureExpansion();
		console.log(JSON.stringify({ ok: true, output: frames }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
