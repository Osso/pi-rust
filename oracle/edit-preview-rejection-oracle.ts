import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api = {createEditToolDefinition(cwd:string): {renderCall(args:unknown,theme:unknown,context:unknown):unknown}};
void runOracle(async(module,request)=> {
    const source=process.argv[2];
    const theme=await import(pathToFileURL(join(dirname(source),"../../modes/interactive/theme/theme.ts")).href);
    theme.initTheme("dark");
    const output=[];
    for(const value of requireCases(request)) {
        const input=value as {path:string};
        const rejection=Promise.withResolvers<unknown>();
        const capture=(error:unknown)=>rejection.resolve(error);
        process.once("unhandledRejection",capture);
        const args={path:input.path,edits:[{oldText:"before",newText:"after"}]};
        try {
            (module as Api).createEditToolDefinition("/repo").renderCall(args,theme.theme,{
                args,toolCallId:"preview-rejection",invalidate:()=>{},lastComponent:undefined,state:{},cwd:"/repo",
                executionStarted:false,argsComplete:true,isPartial:false,expanded:false,showImages:false,isError:false,
            });
        } catch { /* Capture the separate rejected computeEditsDiff Promise, not this synchronous callback. */ }
        const error=await Promise.race([rejection.promise,new Promise(resolve=>setTimeout(()=>resolve(null),500))]);
        process.removeListener("unhandledRejection",capture);
        if(!(error instanceof Error)) output.push({unhandled:false});
        else output.push({unhandled:true,name:error.name,message:error.message,constructor:error.constructor.name,string:String(error)});
    }
    return output;
}).catch((error:unknown)=> {process.stderr.write(`edit-preview rejection oracle failed: ${String(error)}\n`);process.exitCode=1;});
