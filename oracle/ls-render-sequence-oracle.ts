import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Component={render(width:number):string[]};
type Step={kind:"call"|"result";args?:Record<string,unknown>;result?:{content:unknown[];details?:Record<string,unknown>};number?:string;expanded?:boolean;showImages?:boolean};
type Api={createLsToolDefinition(cwd:string):{renderCall(args:unknown,theme:unknown,context:unknown):Component;renderResult(result:unknown,options:unknown,theme:unknown,context:unknown):Component}};
const themePromise=(async()=> {
    const source=process.argv[2];
    const theme=await import(pathToFileURL(join(dirname(source),"../../modes/interactive/theme/theme.ts")).href);
    theme.initTheme("dark");return theme.theme;
})();
void runOracle(async(module,request)=> {
    const theme=await themePromise;const output=[];
    for(const value of requireCases(request)) {
        const steps=value as Step[];
        const definition=(module as Api).createLsToolDefinition("/repo");
        let call:Component|undefined;let result:Component|undefined;
        const frames=[];
        for(const step of steps) {
            if(step.number !== undefined) {
                if(step.kind === "call") step.args={...step.args,limit:Number(step.number)};
                else step.result={...step.result!,details:{...step.result?.details,entryLimitReached:Number(step.number)}};
            }
            const previous=step.kind === "call"?call:result;
            const context={args:step.args,toolCallId:"ls-sequence",invalidate:()=>{},lastComponent:previous,state:{},cwd:"/repo",executionStarted:true,argsComplete:true,isPartial:false,expanded:step.expanded ?? false,showImages:step.showImages ?? false,isError:false};
            const component=step.kind === "call"?definition.renderCall(step.args,theme,context):definition.renderResult(step.result,{expanded:context.expanded,isPartial:false},theme,context);
            const reused=component === previous;
            if(step.kind === "call") call=component;else result=component;
            frames.push({lines:component.render(100),reused});
        }
        output.push(frames);
    }
    return output;
}).catch((error:unknown)=> {process.stderr.write(`ls-render sequence failed: ${String(error)}\n`);process.exitCode=1;});
