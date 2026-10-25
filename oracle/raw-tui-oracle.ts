import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { selectApprovedModule } from "./approved-delta.mjs";

async function loadEditor(source: string) {
    process.env.PI_APPROVED_DELTA = "1";
    const editorPath = selectApprovedModule(path.join(source, "components/editor.ts"), "packages/tui/src/components/editor.ts");
    return import(pathToFileURL(editorPath).href);
}

async function main(): Promise<void> {
	const source = process.argv[2]!;
	const utils = await import(pathToFileURL(path.join(source, "utils.ts")).href);
	const { Input } = await import(pathToFileURL(path.join(source, "components/input.ts")).href);
	const { Text } = await import(pathToFileURL(path.join(source, "components/text.ts")).href);
	const native = (units: number[]): string => String.fromCharCode(...units);
	
	for await (const line of createInterface({ input: process.stdin })) {
		try {
			const request = JSON.parse(line);
			let output: unknown;
			if (request.operation === "stdin_native") {
                const {StdinBuffer} = await import(pathToFileURL(path.join(source,"stdin-buffer.ts")).href);
                const buffer = new StdinBuffer();
                const events: [string,string][] = [];
                buffer.on("data", (data: string)=>events.push(["data",data]));
                buffer.on("paste", (data: string)=>events.push(["paste",data]));
                const states = request.chunks.map((units: number[]) => {
                    buffer.process(native(units));
                    return [events.splice(0),buffer.getBuffer()];
                });
                output = [states,buffer.flush()];
                buffer.destroy();
            } else if (request.operation === "keybindings_native") {
                const { KeybindingsManager } = await import(pathToFileURL(path.join(source,"keybindings.ts")).href);
                const raw = native(request.units);
                const first = raw+"first";
                const second = raw+"second";
                const key = "ctrl+"+raw;
                const manager = new KeybindingsManager({[first]:{defaultKeys:[key,"enter"],description:raw},[second]:{defaultKeys:"escape",description:raw}}, {[first]:[key,key],[second]:[key]});
                output = [manager.getKeys(first),[manager.getDefinition(first).defaultKeys,manager.getDefinition(first).description],manager.getConflicts().map((item: {key:string;keybindings:string[]})=>[item.key,item.keybindings]),Object.entries(manager.getResolvedBindings()).map(([id,keys])=>[id,typeof keys === "string"?[keys]:keys]),Object.entries(manager.getUserBindings()),manager.matches(raw,first)];
            } else if (request.operation === "markdown_tokens") {
                const { Marked,Tokenizer } = await import(pathToFileURL(path.join(source,"../../../node_modules/marked/lib/marked.esm.js")).href);
                class Strict extends Tokenizer {
                    override del(src: string) {
                        const match = /^(~~)(?=[^\s~])((?:\\.|[^\\])*?(?:\\.|[^\s~\\]))\1(?=[^~]|$)/.exec(src);
                        if (!match) return undefined;
                        return {type:"del",raw:match[0],text:match[2],tokens:this.lexer.inlineTokens(match[2])};
                    }
                }
                output = new Marked({tokenizer:new Strict()}).lexer(native(request.units));
            } else if (request.operation === "markdown") {
                const module = await import(pathToFileURL(path.join(source, "components/markdown.ts")).href);
                const images = await import(pathToFileURL(path.join(source, "terminal-image.ts")).href);
                images.setCapabilities({images:null,trueColor:false,hyperlinks:false});
                const identity = (text: string): string => text;
                const theme = Object.fromEntries(["heading","link","linkUrl","code","codeBlock","codeBlockBorder","quote","quoteBorder","hr","listBullet","bold","italic","strikethrough","underline"].map(key => [key,identity]));
                const text = native(request.units);
                const md = new module.Markdown(text,0,0,theme);
                output = [md.render(request.width),md.render(request.width)];
                images.resetCapabilitiesCache();
            } else if (request.operation === "utils") {
				const text = native(request.units);
				output = {
					width: utils.visibleWidth(text),
					wrap: utils.wrapTextWithAnsi(text, request.width),
					truncate: utils.truncateToWidth(text, request.width),
					slice: utils.sliceByColumn(text, request.start, request.width, true),
					segments: utils.extractSegments(text, request.start, request.start + 1, request.width, true),
					normalize: utils.normalizeTerminalOutput(text),
				};
			} else if (request.operation === "text") {
				const text = new Text(native(request.units), request.paddingX ?? 0, request.paddingY ?? 0);
				if (request.background) text.setCustomBgFn((value: string) => `\x1b[44m${value}\x1b[0m`);
				output = [text.render(request.width), text.render(request.width)];
			} else if (request.operation === "input") {
				const input = new Input({ mask: request.mask ?? false });
				const submitted: string[] = [];
				input.onSubmit = (value: string) => submitted.push(value);
				const states: unknown[] = [];
				for (const action of request.actions) {
					if (action.set) input.setValue(native(action.set));
					if (action.key) input.handleInput(native(action.key));
					states.push({ value: input.getValue(), cursor: (input as unknown as { cursor: number }).cursor, lines: input.render(request.width) });
				}
				output = { states, submitted };
			} else if (request.operation === "image_values") {
                output = await captureImageValues(source,native(request.units),request.protocol,request.width);
            } else if (request.operation === "image_dimensions") {
                output = await captureImageDimensions(source,native(request.units));
            } else if (request.operation === "kitty_encode") {
                const module = await import(pathToFileURL(path.join(source, "terminal-image.ts")).href);
                output = module.encodeKitty(native(request.units));
            } else if (request.operation === "editor_actions") {
                output = await captureEditorActions(source,request.actions,request.width);
            } else if (request.operation === "editor_set") {
                output = await captureEditorSet(source,native(request.units),request.width);
            } else if (request.operation === "word_navigation") {
                output = await captureWordNavigation(source,native(request.units),utils);
            } else if (request.operation === "autocomplete_commands") {
                output = await captureAutocompleteCommands(source,native(request.units));
            } else if (request.operation === "autocomplete_files") {
                const module = await import(pathToFileURL(path.join(source, "autocomplete.ts")).href);
                const provider = new module.CombinedAutocompleteProvider([], request.base, null);
                const prefix = native(request.units);
                const result = await provider.getSuggestions([prefix],0,prefix.length,{signal:new AbortController().signal});
                output = result ? [result.items.map((item: {value:string;label:string;description?:string}) => [item.value,item.label,item.description ?? null]),result.prefix] : null;
            } else if (request.operation === "autocomplete_apply") {
                const module = await import(pathToFileURL(path.join(source, "autocomplete.ts")).href);
                const provider = new module.CombinedAutocompleteProvider([], ".", null);
                const item = native(request.item);
                const label = request.itemLabel ? native(request.itemLabel) : item;
                const result = provider.applyCompletion([native(request.units)], 0, request.cursor, {value:item,label},native(request.prefix));
                output = [result.lines,result.cursorLine,result.cursorCol];
            } else if (request.operation === "settings") {
                output = await captureSettings(source, native(request.units), request.width, request.search);
            } else if (request.operation === "input_route") {
                output = await captureInputRoute(source);
            } else if (request.operation === "terminal_colors") {
                const module = await import(pathToFileURL(path.join(source, "terminal-colors.ts")).href);
                const input = native(request.units);
                const color = module.parseOsc11BackgroundColor(input);
                output = [module.isOsc11BackgroundColorResponse(input), color ? [color.r,color.g,color.b] : null];
            } else if (request.operation === "key_identity") {
                const module = await import(pathToFileURL(path.join(source, "keys.ts")).href);
                const input = native(request.units);
                const key = native(request.key);
                output = [module.matchesKey(input, key), module.Key.ctrl(key), module.Key.alt(key), module.isKeyRelease(input), module.isKeyRepeat(input)];
            } else if (request.operation === "keys") {
                const module = await import(pathToFileURL(path.join(source, "keys.ts")).href);
                const input = native(request.units);
                output = [module.decodePrintableKey(input) ?? null, module.parseKey(input) ?? null, module.isKeyRelease(input), module.isKeyRepeat(input)];
            } else if (request.operation === "fuzzy") {
                const module = await import(pathToFileURL(path.join(source, "fuzzy.ts")).href);
                const query = native(request.query);
                const items = request.items.map(native);
                output = [items.map((item: string) => { const result = module.fuzzyMatch(query, item); return [result.matches, result.score]; }), module.fuzzyFilter(items, query, (item: string) => item)];
            } else if (request.operation === "select") {
                output = await captureSelect(source, native(request.units), request.width);
            } else if (request.operation === "components") {
                output = await captureComponents(source, native(request.units), request.width, request.padding);
            } else if (request.operation === "frame") {
                output = await captureFrame(source, native(request.units));
            } else if (request.operation === "terminal_title") {
                const module = await import(pathToFileURL(path.join(source, "terminal.ts")).href);
                const terminal = new module.ProcessTerminal();
                const writes: string[] = [];
                const original = process.stdout.write;
                try {
                    process.stdout.write = ((value: string) => { writes.push(value); return true; }) as typeof original;
                    terminal.setTitle(native(request.units));
                } finally {
                    process.stdout.write = original;
                }
                const child = spawnSync(process.execPath, ["-e", "process.stdout.write(JSON.parse(process.argv[1]))", JSON.stringify(writes[0])]);
                if (child.status !== 0) throw new Error(child.stderr.toString());
                output = [writes, child.stdout.toString("hex")];
            } else if (request.operation === "terminal_bytes") {
                const child = spawnSync(process.execPath, ["-e", "process.stdout.write(JSON.parse(process.argv[1]))", JSON.stringify(native(request.units))]);
                if (child.status !== 0) throw new Error(child.stderr.toString());
                output = child.stdout.toString("hex");
            } else {
				throw new Error(`Unknown operation: ${request.operation}`);
			}
			// Preserve the source's native string serialization; no scalar conversion.
			process.stdout.write(`${JSON.stringify({ ok: true, json: JSON.stringify(output) })}\n`);
		} catch (error) {
			process.stdout.write(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
		}
	}
}
void main();

class RawTerminal {
    columns = 12;
    rows = 5;
    kittyProtocolActive = true;
    writes: string[] = [];
    private onInput?: (value: string) => void;
    start(onInput: (value: string) => void): void { this.onInput = onInput; this.write("\x1b[?2004h"); }
    sendInput(value: string): void { this.onInput?.(value); }
    stop(): void { this.write("\x1b[?2004l"); }
    async drainInput(): Promise<void> {}
    write(value: string): void { this.writes.push(value); }
    moveBy(lines: number): void {
        if (lines > 0) this.write(`\x1b[${lines}B`);
        else if (lines < 0) this.write(`\x1b[${-lines}A`);
    }
    hideCursor(): void { this.write("\x1b[?25l"); }
    showCursor(): void { this.write("\x1b[?25h"); }
    clearLine(): void { this.write("\x1b[K"); }
    clearFromCursor(): void { this.write("\x1b[J"); }
    clearScreen(): void { this.write("\x1b[2J\x1b[H"); }
    setTitle(): void {}
    setProgress(): void {}
    takeWrites(): string[] { return this.writes.splice(0); }
}

async function captureFrame(source: string, raw: string): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "tui.ts")).href);
    const images = await import(pathToFileURL(path.join(source, "terminal-image.ts")).href);
    images.setCapabilities({ images: null, trueColor: false, hyperlinks: false });
    const terminal = new RawTerminal();
    const tui = new module.TUI(terminal);
    const lines = [`x${raw}`, "tail"];
    const frames: string[][] = [];
    tui.addChild({ render: () => lines, invalidate() {} });
    const flush = (): void => {
        tui.renderPendingState();
        frames.push(terminal.takeWrites());
    };
    tui.start();
    flush();
    lines[0] = `y${raw}`;
    tui.requestRender();
    flush();
    const handle = tui.showOverlay({ render: () => [raw], invalidate() {} }, { width: 4, row: 0, col: 2 });
    flush();
    handle.hide();
    flush();
    const cell = tui.createFixedCell();
    cell.place(0, 1);
    const updated = cell.update("\ud800z\udc00");
    frames.push(terminal.takeWrites());
    tui.stop();
    frames.push(terminal.takeWrites());
    const stop = frames.pop()!.join("");
    return [frames, stop, updated];
}

async function captureComponents(source: string, raw: string, width: number, padding: number): Promise<unknown> {
    const load = (name: string) => import(pathToFileURL(path.join(source, name)).href);
    const boxApi = await load("components/box.ts");
    const truncatedApi = await load("components/truncated-text.ts");
    const borderApi = await load("components/dynamic-border.ts");
    const spacerApi = await load("components/spacer.ts");
    const rootApi = await load("root-compositor.ts");
    const loaderApi = await load("components/loader.ts");
    const style = (value: string): string => `\x1b[44m${value}\x1b[0m`;
    const boxed = new boxApi.Box(padding, padding, style);
    boxed.addChild({ render: () => [raw], invalidate() {} });
    const truncated = new truncatedApi.TruncatedText(raw, padding, padding);
    const border = new borderApi.DynamicBorder((value: string) => raw + value);
    const spacer = new spacerApi.Spacer(2);
    const entry = () => ({ component: { render: () => [raw], invalidate() {} } });
    const root = new rootApi.RootCompositor({ getHeight: () => 5, flow: [entry()], bottom: [entry()] });
    const loader = new loaderApi.Loader({ requestComponentRender() {} }, style, style, raw, { frames: [raw] });
    const firstLoader = loader.render(width);
    loader.setMessage(raw + "x");
    const output = {
        box_lines: boxed.render(width), box_cached: boxed.render(width),
        truncated: truncated.render(width), border: border.render(width), spacer: spacer.render(width),
        root: root.render(width), loader: firstLoader, loader_updated: loader.render(width),
    };
    loader.stop();
    return output;
}

async function captureSelect(source: string, raw: string, width: number): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "components/select-list.ts")).href);
    const identity = (value: string): string => value;
    const theme = { selectedPrefix: identity, selectedText: identity, description: identity, scrollInfo: identity, noMatch: identity };
    const items = [0,1,2].map((index) => ({ value: raw + index, label: raw + "label" + index, description: raw + "\r\n desc " + raw }));
    const list = new module.SelectList(items, 2, theme);
    const callbacks: string[] = [];
    list.onSelect = (item: { value: string }) => callbacks.push(item.value);
    const lines = [list.render(width)];
    list.handleInput("\x1b[B");
    list.handleInput("\r");
    lines.push(list.render(width));
    list.setFilter(raw);
    lines.push(list.render(width));
    const selected = list.getSelectedItem()?.value ?? null;
    list.setFilter(raw + "missing");
    lines.push(list.render(width));
    return [lines, selected, callbacks];
}

async function captureInputRoute(source: string): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "tui.ts")).href);
    const inputApi = await import(pathToFileURL(path.join(source, "components/input.ts")).href);
    const terminal = new RawTerminal();
    const tui = new module.TUI(terminal);
    const input = new inputApi.Input();
    const submitted: string[] = [];
    const seen: string[] = [];
    input.onSubmit = (value: string) => submitted.push(value);
    tui.addChild(input);
    tui.setFocus(input);
    tui.addInputListener((value: string) => {
        seen.push(value);
        if (value === "x") return { data: "\ud800" };
        if (value === "y") return { data: "\udc00" };
    });
    tui.start();
    for (const value of ["x", "y", "\ud800", "z", "\udc00", "\r"]) terminal.sendInput(value);
    tui.stop();
    return [input.getValue(), submitted, seen];
}

async function captureSettings(source: string, raw: string, width: number, search: boolean): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "components/settings-list.ts")).href);
    const identity = (value: string): string => value;
    const theme = { label: identity, value: identity, description: identity, cursor: "→ ", hint: identity };
    const changes: string[][] = [];
    let cancelled = 0;
    const items = [0,1].map((index) => ({ id: raw + index, label: raw + "label" + index, description: raw + " desc", currentValue: raw + "v0", values: [raw + "v0",raw + "v1"] }));
    const list = new module.SettingsList(items, 2, theme, (id: string, value: string) => changes.push([id,value]), () => cancelled++, { enableSearch: search });
    const renders = [list.render(width)];
    list.updateValue(raw + "0", raw + "external");
    renders.push(list.render(width));
    for (const key of ["\r", raw, "\x1b[B", "\r", "\x1b"]) {
        list.handleInput(key);
        renders.push(list.render(width));
    }
    return [renders, changes, cancelled, list.selectedIndex];
}

async function captureAutocompleteCommands(source: string, raw: string): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "autocomplete.ts")).href);
    const captured: string[] = [];
    const command = { name: raw + "cmd", description: raw + "description", argumentHint: raw + "hint", getArgumentCompletions: (argument: string) => { captured.push(argument); return [{ value: raw + "arg", label: raw + "arglabel" }]; } };
    const provider = new module.CombinedAutocompleteProvider([command,{name:raw + "other"}], ".", null);
    const results: unknown[] = [];
    for (const query of ["/" + raw.slice(0,1),"/" + raw + "cmd","/" + raw + "cmd " + raw + " arg"]) {
        const result = await provider.getSuggestions([query],0,query.length,{signal:new AbortController().signal});
        const suggestions = result ? [result.items.map((item: {value:string;label:string;description?:string}) => [item.value,item.label,item.description ?? null]),result.prefix] : null;
        results.push([suggestions,provider.shouldTriggerFileCompletion([query],0,query.length)]);
    }
    return [results,captured];
}

async function captureWordNavigation(source: string, raw: string, utils: { getWordSegmenter(): Intl.Segmenter }): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "word-navigation.ts")).href);
    const segments = [...utils.getWordSegmenter().segment(raw)].map((segment) => [segment.segment,segment.isWordLike]);
    const positions = Array.from({length:raw.length + 1},(_,cursor) => [module.findWordBackward(raw,cursor),module.findWordForward(raw,cursor)]);
    const seen: string[] = [];
    const atomic: string[] = [];
    const options = { segment: (value: string) => { seen.push(value); return [{segment:value,isWordLike:true}]; }, isAtomicSegment: (value: string) => { atomic.push(value); return value.includes("\ud800"); } };
    const backward = module.findWordBackward(raw,1,options);
    const forward = module.findWordForward(raw,1,options);
    return [segments,positions,[backward,forward,seen,atomic]];
}

async function captureEditorSet(source: string, raw: string, width: number): Promise<unknown> {
    const editorApi = await loadEditor(source);
    const tuiApi = await import(pathToFileURL(path.join(source, "tui.ts")).href);
    const terminal = new RawTerminal();
    terminal.rows = 24;
    terminal.columns = 80;
    const tui = new tuiApi.TUI(terminal);
    const identity = (value: string): string => value;
    const theme = { borderColor: identity, selectList: { selectedPrefix: identity, selectedText: identity, description: identity, scrollInfo: identity, noMatch: identity } };
    const editor = new editorApi.Editor(tui,theme);
    editor.setText(raw);
    const cursor = editor.getCursor();
    return [editor.getText(),editor.getExpandedText(),cursor.line,cursor.col,editor.render(width)];
}

async function captureEditorActions(source: string, actions: Array<{set?:number[];key?:number[];insert?:number[];history?:number[][]}>, width: number): Promise<unknown> {
    const editorApi = await loadEditor(source);
    const tuiApi = await import(pathToFileURL(path.join(source, "tui.ts")).href);
    const terminal = new RawTerminal();
    terminal.rows = 24;
    terminal.columns = 80;
    const tui = new tuiApi.TUI(terminal);
    const identity = (value: string): string => value;
    const theme = { borderColor: identity, selectList: { selectedPrefix: identity, selectedText: identity, description: identity, scrollInfo: identity, noMatch: identity } };
    const editor = new editorApi.Editor(tui,theme);
    const changes: string[] = [];
    const submissions: string[] = [];
    editor.onChange = (value: string) => changes.push(value);
    editor.onSubmit = (value: string) => submissions.push(value);
    const states: unknown[] = [];
    const native = (units: number[]): string => String.fromCharCode(...units);
    for (const action of actions) {
        if (action.set) editor.setText(native(action.set));
        if (action.key) editor.handleInput(native(action.key));
        if (action.insert) editor.insertTextAtCursor(native(action.insert));
        if (action.history) editor.setHistory(action.history.map(native));
        const lines = editor.render(width);
        const cursor = editor.getCursor();
        states.push({text:editor.getText(),expanded:editor.getExpandedText(),cursor:[cursor.line,cursor.col],lines,history:editor.getHistory()});
    }
    return {states,changes,submissions};
}

async function captureImageValues(source: string, raw: string, protocol: string, width: number): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "terminal-image.ts")).href);
    const imageApi = await import(pathToFileURL(path.join(source, "components/image.ts")).href);
    module.setCapabilities({images:protocol === "none" ? null : protocol,trueColor:true,hyperlinks:true});
    module.setCellDimensions({widthPx:9,heightPx:18});
    const dimensions = {widthPx:18,heightPx:36};
    const kitty = module.encodeKitty(raw,{columns:2,rows:3,imageId:9,moveCursor:false});
    const iterm = module.encodeITerm2(raw,{width:raw,height:raw + "%",name:raw + ".png",preserveAspectRatio:false,inline:false});
    const link = module.hyperlink(raw,"https://host/" + raw);
    const mime = "image/" + raw;
    const filename = raw + ".png";
    const fallback = module.imageFallback(mime,dimensions,filename);
    const result = module.renderImage(raw,dimensions,{maxWidthCells:6,maxHeightCells:3,imageId:9,moveCursor:false});
    const rendered = result ? [result.sequence,result.rows,result.imageId ?? null] : null;
    const seen: string[] = [];
    const image = new imageApi.Image(raw,mime,{fallbackColor:(value: string) => { seen.push(value); return raw + value + raw; }},{filename,imageId:9},dimensions);
    const first = image.render(width);
    const cached = image.render(width);
    image.invalidate();
    const invalidated = image.render(width);
    return [kitty,iterm,link,fallback,[module.deleteKittyImage(9),module.deleteAllKittyImages()],rendered,[first,cached,invalidated,image.getImageId() ?? null],seen];
}

async function captureImageDimensions(source: string, raw: string): Promise<unknown> {
    const module = await import(pathToFileURL(path.join(source, "terminal-image.ts")).href);
    const imageApi = await import(pathToFileURL(path.join(source, "components/image.ts")).href);
    module.setCapabilities({images:"kitty",trueColor:true,hyperlinks:true});
    module.setCellDimensions({widthPx:9,heightPx:18});
    const result = module.getImageDimensions(raw,"image/png");
    const dimensions = result ? [result.widthPx,result.heightPx] : null;
    const image = new imageApi.Image(raw,"image/png",{fallbackColor:(value: string) => value},{imageId:9});
    return [dimensions,image.render(10)];
}
