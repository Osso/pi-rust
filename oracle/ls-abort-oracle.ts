import { setImmediate } from "node:timers/promises";
import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input={phase:string;reject?:boolean;preabort?:boolean};
type Api={createLsToolDefinition(cwd:string,options:unknown):{execute(id:string,args:unknown,signal:AbortSignal):Promise<unknown>}};
void runOracle(async(module,request)=> {
    const output=[];
    for(const value of requireCases(request)) {
        const input=value as Input;
        const events:string[]=[];
        const gate=Promise.withResolvers<void>();const started=Promise.withResolvers<void>();const done=Promise.withResolvers<void>();
        const controller=new AbortController();
        const operation=async(phase:string)=> {
            events.push(`${phase}:start`);
            if(phase === input.phase) {
                started.resolve();await gate.promise;
                if(input.reject) {
                    events.push(`${phase}:reject`);
                    if(phase !== "stat-a") done.resolve();
                    throw new Error("operation rejected");
                }
            }
            events.push(`${phase}:end`);
            if(phase === "stat-b") done.resolve();
        };
        const tool=(module as Api).createLsToolDefinition("/repo",{operations:{
            exists:async()=> {await operation("exists");return true;},
            stat:async(path:string)=> {const phase=path === "/repo/list"?"stat-root":path.endsWith("/a")?"stat-a":"stat-b";await operation(phase);return {isDirectory:()=>phase === "stat-root"};},
            readdir:async()=> {await operation("readdir");return ["a","b"];},
        }});
        if(input.preabort) controller.abort({ignored:"reason"});
        const result=tool.execute("ls-abort",{path:"list"},controller.signal).then(value=>({ok:true,value}),error=>({ok:false,error:error.message}));
        if(input.preabort) {output.push({result:await result,events,continued:true});continue;}
        await started.promise;
        controller.abort({ignored:"reason"});
        const observed=await result;
        const before=[...events];
        gate.resolve();
        const continued=await Promise.race([done.promise.then(()=>true),new Promise<boolean>(resolve=>setTimeout(()=>resolve(false),300))]);
        await setImmediate();
        output.push({result:observed,before,events,continued});
    }
    return output;
}).catch((error:unknown)=> {process.stderr.write(`ls-abort oracle failed: ${String(error)}\n`);process.exitCode=1;});
