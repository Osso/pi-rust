import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const root = process.argv[2];
if (!root) throw new Error("coding-agent source root required");
const { InteractiveMode } = await import(pathToFileURL(`${root}/modes/interactive/interactive-mode.ts`).href);
const { Text, Container } = await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const units = (text: string) => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
for await (const line of createInterface({ input: process.stdin })) {
	try {
		const request = JSON.parse(line);
		const failure = Object.assign(new TypeError(String.fromCharCode(...request.error)), { code: "HEADER_FAILURE" });
		const builtInHeader = new Text("", 0, 0);
		const headerContainer = new Container();
		headerContainer.addChild(builtInHeader);
		const receiver = Object.assign(Object.create(InteractiveMode.prototype), { builtInHeader, headerContainer, toolOutputExpanded: true, ui: { requestRender() {} } });
		let caught: unknown;
		try {
			receiver.setExtensionHeader(() => {
				if (request.stage === "factory") throw failure;
				return Object.assign(new Text("never mounted", 0, 0), { setExpanded() { throw failure; } });
			});
		} catch (error) { caught = error; }
		if (!(caught instanceof TypeError) || !("code" in caught)) throw new Error("frozen header must throw its original coded TypeError");
		console.log(JSON.stringify({ ok: true, output: { message: units(caught.message), name: caught.name,
			code: caught.code, typeError: caught instanceof TypeError, sameError: caught === failure,
			frame: headerContainer.render(80).map(units) } }));
	} catch (error) {
		console.log(JSON.stringify({ ok: false, error: String(error) }));
	}
}
