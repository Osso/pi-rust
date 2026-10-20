import {pathToFileURL} from 'node:url';
import fs from 'node:fs';
const example=import(pathToFileURL(process.argv[2]).href);
async function run(request:any) {
 process.chdir(request.cwd);
 const tools:any[]=[];const hooks:any={};const trace:any[]=[];
 let flag:any=undefined;
 const ui={theme:{fg:(_color:string,text:string)=>text},setStatus:(key:string,text:string)=>trace.push(['status',key,text]),notify:(text:string,level:string)=>trace.push(['notify',text,level])};
 (await example).default({registerFlag:(name:string,options:any)=>trace.push(['flag',name,options]),getFlag:()=>flag,registerTool:(tool:any)=>tools.push(tool),on:(name:string,handler:any)=>hooks[name]=handler});
 const metadata=tools.map(({name,label,description,parameters})=>({name,label,description,parameters}));
 // Factory runs before the CLI flag is available; prove pre-start local execution.
 const before=await tools[0].execute('before',{path:'read.txt'});
 flag=request.flag;
 let startError:any=null;
 try {await hooks.session_start({type:'session_start'},{ui});} catch(e:any){startError=e.message;}
 const prompt=`Current working directory: ${request.cwd}\nCurrent working directory: ${request.cwd}`;
 const changed=await hooks.before_agent_start({systemPrompt:prompt});
 const results=[];
 for(const call of request.calls) {
  try {results.push({result:await tools.find(t=>t.name===call.tool).execute('call',call.params)});}catch(e:any){results.push({error:e.message});}
 }
 const userOps=await hooks.user_bash({type:'user_bash',command:'ignored'});
 const user=[];
 if(userOps) {
  for(const mode of ['success','exit','timeout','abort','preabort']) {
   let output='';const controller=new AbortController();
   if(mode==='preabort')controller.abort();
   try {
    const value=await userOps.operations.exec(mode==='timeout'?'exec sleep 30':mode==='abort'?"exec python3 -c 'import time; print(\"ready\",flush=True); time.sleep(30)'":mode==='exit'?"printf code; exit 7":"printf 'user output'",request.cwd,{timeout:mode==='timeout'?1:undefined,signal:controller.signal,onData:(bytes:Buffer)=>{output+=bytes.toString();if(mode==='abort')controller.abort();}});
    user.push({mode,output,result:value});
   }catch(e:any){user.push({mode,output,error:e.message});}
  }
 }
 return {metadata,before,trace,startError,prompt:changed?.systemPrompt??null,results,user};
}
async function main(){
 let pending='';process.stdin.setEncoding('utf8');
 for await(const chunk of process.stdin){pending+=chunk;let end:number;while((end=pending.indexOf('\n'))!==-1){const line=pending.slice(0,end);pending=pending.slice(end+1);console.log(JSON.stringify(await run(JSON.parse(line))));}}
}
main().catch(e=>{console.error(e);process.exit(1);});
