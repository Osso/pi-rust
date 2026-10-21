// Complete pinned module, import-boundary faux backends/transport/clock; real extension loader/runtime.
import {readFileSync} from 'node:fs';import {createRequire} from 'node:module';import {dirname,resolve} from 'node:path';import {pathToFileURL} from 'node:url';import {webcrypto} from 'node:crypto';import vm from 'node:vm';import {createInterface} from 'node:readline';
const file=process.argv[2],root=resolve(dirname(file),'../../../../..'),require=createRequire(resolve(root,'package.json')),ts=require('typescript');
const js=ts.transpileModule(readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
async function main(){
 const load=(p:string)=>import(pathToFileURL(resolve(root,'packages/coding-agent/src',p)).href);
 const [{createExtensionRuntime,loadExtensionFromFactory},{createEventBus}]=await Promise.all([load('core/extensions/loader.ts'),load('core/event-bus.ts')]);
 for await(const line of createInterface({input:process.stdin})){
  const r=JSON.parse(line),calls:any[]=[],events:any[][]=[];let current:any[]=[],finish:()=>void=()=>{};
  const event={type:'error',reason:'aborted',error:{role:'assistant',api:'fixture-api',provider:'fixture',model:'fixture',content:[],usage:{input:0,output:0,cacheRead:0,cacheWrite:0,totalTokens:0,cost:{input:0,output:0,cacheRead:0,cacheWrite:0,total:0}},stopReason:'aborted',errorMessage:'backend event',timestamp:12}};
  const backend=(name:string)=>({streamSimple:(model:any,context:any,o:any)=>{calls.push({type:'backend',backend:name,model,context,apiKey:o.apiKey,headers:o.headers});return(async function*(){yield event;})();}});
  const sandbox:any={exports:{},Error,URL,URLSearchParams,TextEncoder,btoa,Date:{now:()=>1700000000000},crypto:{getRandomValues:(a:Uint8Array)=>{a.set(Array.from({length:a.length},(_,i)=>i));return a;},subtle:webcrypto.subtle,randomUUID:()=> '00000000-0000-4000-8000-000000000000'},fetch:async(url:string,o:any)=>{calls.push({type:'fetch',url,method:o.method,headers:o.headers,body:o.body,signal:!!o.signal});if(r.fetchError)throw new Error(r.fetchError);const token=url.endsWith('/oauth/token')?{access_token:'FAKE-access',refresh_token:'FAKE-refresh',created_at:1700000000,expires_in:3600}:{token:'FAKE-direct',headers:{'X-Gitlab':'fixture',Authorization:'replace'}};return{ok:(r.status??200)<300,status:r.status??200,text:async()=>r.body??JSON.stringify(token),json:async()=>r.body?JSON.parse(r.body):token};},require:(name:string)=>{if(name==='@earendil-works/pi-ai/compat')return{anthropicMessagesApi:()=>backend('anthropic'),openAIResponsesApi:()=>backend('openai'),createAssistantMessageEventStream:()=>({push:(e:any)=>current.push(e),end:()=>finish()})};throw Error('Unexpected import '+name);}};
  vm.runInNewContext(js,sandbox,{filename:file});
  const runtime=createExtensionRuntime();await loadExtensionFromFactory(sandbox.exports.default,'/fixture',createEventBus(),runtime,'<example:gitlab>');const config=runtime.pendingProviderRegistrations[0].config;
  if(r.kind==='metadata'){console.log(JSON.stringify({metadata:{id:'gitlab-duo',baseUrl:config.baseUrl,apiKey:config.apiKey,api:config.api,models:config.models,oauthName:config.oauth.name}}));continue;}
  if(r.kind==='login'||r.kind==='refresh'){
   try{const credential=r.kind==='login'?await config.oauth.login({onAuth:(v:any)=>calls.push({type:'auth',...v}),onPrompt:async(v:any)=>{calls.push({type:'prompt',...v});return r.code??'http://localhost/callback?code=FAKE-code&state=ignored';}}):await config.oauth.refreshToken({refresh:'old-refresh',access:'old-access',expires:0},new AbortController().signal);console.log(JSON.stringify({calls,credential,auth:config.oauth.getApiKey(credential)}));}catch(e:any){console.log(JSON.stringify({calls,error:e.message}));}continue;
  }
  for(const index of r.indices){current=[];const done=new Promise<void>(resolve=>finish=resolve);const model={...config.models[index],api:config.api,provider:'gitlab-duo',baseUrl:config.baseUrl};if(r.unknown)model.id='unknown';config.streamSimple(model,{messages:[]},{apiKey:r.missingKey?undefined:'FAKE-gitlab',headers:{ignored:'caller'}});await done;events.push(current);}
  console.log(JSON.stringify({calls,events}));
 }
}
main().catch(e=>{console.error(e);process.exit(1)});
