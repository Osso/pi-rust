// Batch H: unchanged pinned factory, real extension loader/runner and pi-tui Text.
// Host fixture only; invoked by the guarded Rust test runner, never directly.
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { readFile, writeFile } from "node:fs/promises";

async function main() {
 const source = process.argv[2];
 const packages = resolve(dirname(source), "../../..");
 const load = (path: string) => import(pathToFileURL(resolve(packages, path)).href);
 const [{default: factory}, loader, runnerModule, session, auth, utils, events, themeModule] = await Promise.all([
  import(pathToFileURL(source).href), load("coding-agent/src/core/extensions/loader.ts"),
  load("coding-agent/src/core/extensions/runner.ts"), load("coding-agent/src/core/session-manager.ts"),
  load("coding-agent/src/core/auth-storage.ts"), load("coding-agent/test/model-runtime-test-utils.ts"),
  load("coding-agent/src/core/event-bus.ts"), load("coding-agent/src/modes/interactive/theme/theme.ts"),
 ]);
 async function run(request: any) {
  process.chdir(request.cwd);
  const runtime = loader.createExtensionRuntime();
  const extension = await loader.loadExtensionFromFactory(factory, request.cwd, events.createEventBus(), runtime, source);
  const registry = await utils.createInMemoryModelRegistry(auth.AuthStorage.inMemory());
  const runner = new runnerModule.ExtensionRunner([extension], runtime, request.cwd, session.SessionManager.inMemory(request.cwd), registry);
  const theme = new themeModule.Theme({...request.colors, thinkingXhigh:"#ffffff", text:"#ffffff"}, {selectedBg:"#000000"}, "truecolor");
  // Approved compatibility.md:177 exclusion: String-typed Theme::fg cannot
  // receive unpaired UTF-16 units. Project at that boundary, before Text wraps.
  const fg = theme.fg.bind(theme);
  theme.fg = (role: any, text: string) => fg(role, text.toWellFormed());
  const tools = [...extension.tools.values()].map(({definition: d}: any) => ({
   name:d.name,label:d.label,description:d.description,parameters:d.parameters,renderShell:d.renderShell ?? null,
   promptSnippet:d.promptSnippet ?? null,promptGuidelines:d.promptGuidelines ?? null,
   approvalRequired:d.approvalRequired ?? null,approvalKind:d.approvalKind ?? null,
  }));
  const renders = [];
  for (const item of request.renders) {
   const tool = extension.tools.get(item.tool).definition;
   const context = {args:item.args,toolCallId:"fixture-call",invalidate() {},state:{},cwd:request.cwd,
    executionStarted:true,argsComplete:true,isPartial:item.isPartial,expanded:item.expanded,showImages:false,isError:item.isError ?? false};
   const component = item.result === undefined ? tool.renderCall(item.args,theme,context) :
    tool.renderResult(item.result,{expanded:item.expanded,isPartial:item.isPartial},theme,context);
   renders.push(request.widths.map((width: number) => component.render(width)));
  }
  for (const [name, text] of Object.entries(request.initial)) await writeFile(resolve(request.cwd,name),text as string);
  const executions = [];
  for (const item of request.executions) {
   try {
    const result = await extension.tools.get(item.tool).definition.execute("fixture-call",item.params,undefined,undefined,runner.createContext());
    executions.push({value:result});
   } catch (error: any) { executions.push({error:error.message}); }
  }
  const files: Record<string,string> = {};
  for (const name of request.files) files[name] = await readFile(resolve(request.cwd,name),"utf8");
  return {commands:[...extension.commands.keys()],tools,renders,executions,files};
 }
 process.stdin.setEncoding("utf8");
 let pending = "";
 for await (const chunk of process.stdin) {
  pending += chunk;
  let end: number;
  while ((end = pending.indexOf("\n")) !== -1) {
   const line = pending.slice(0,end); pending = pending.slice(end+1);
   console.log(JSON.stringify(await run(JSON.parse(line))));
  }
 }
 if (pending) throw new Error("Unterminated oracle request");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
