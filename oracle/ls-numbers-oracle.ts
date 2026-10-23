import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Input = {limit?:number; number?:"NaN"|"Infinity"|"-Infinity"|"-0"; rtk?:boolean};
type Api = {createLsToolDefinition(cwd:string,options?:unknown): {execute(id:string,args:unknown):Promise<unknown>}};
void runOracle(async(module,request)=> {
    const output=[];
    for(const value of requireCases(request)) {
        const input=value as Input;
        const calls:unknown[]=[];
        const tool=input.rtk
            ? (module as Api).createLsToolDefinition(String(request.cwd))
            : (module as Api).createLsToolDefinition("/repo",{operations:{
            exists:async(path:string)=> {calls.push(["exists",path]);return true;},
            stat:async(path:string)=> {
                calls.push(["stat",path]);
                if(path.endsWith("/ghost")) throw new Error("stat denied");
                return {isDirectory:()=>path === "/repo/list" || path.endsWith("/dir")};
            },
            readdir:async(path:string)=> {calls.push(["readdir",path]);return ["gamma","alpha","BETA","ghost","dir"];},
        }});
        const args:{path:string;limit?:number}={path:"list"};
        if(input.number !== undefined) args.limit=Number(input.number);
        else if(input.limit !== undefined) args.limit=input.limit;
        const result=await tool.execute("ls-numbers",args);
        output.push({calls,result});
    }
    return output;
}).catch((error:unknown)=> {process.stderr.write(`ls-numbers oracle failed: ${String(error)}\n`);process.exitCode=1;});
