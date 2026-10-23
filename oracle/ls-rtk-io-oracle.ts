import { requireCases, runOracle } from "./coding-agent-utils-oracle-support.ts";

type Api={createLsToolDefinition(cwd:string):{execute(id:string,args:unknown):Promise<unknown>}};
void runOracle(async(module,request)=> {
    const output=[];
    for(const value of requireCases(request)) {
        const input=value as {stdout:string;stderr:string;code:number};
        process.env.PI_FAKE_RTK_IO_CASE=JSON.stringify(input);
        try {output.push({ok:true,value:await (module as Api).createLsToolDefinition(String(request.cwd)).execute("rtk-io",{path:"list"})});}
        catch(error) {if(!(error instanceof Error)) throw error;output.push({ok:false,error:error.message});}
    }
    return output;
}).catch((error:unknown)=> {process.stderr.write(`rtk io oracle failed: ${String(error)}\n`);process.exitCode=1;});
