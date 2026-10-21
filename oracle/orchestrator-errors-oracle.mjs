import {pathToFileURL} from "node:url";
import {mkdirSync,writeFileSync,chmodSync,symlinkSync,existsSync} from "node:fs";
const [root,mode,inputJson]=process.argv.slice(2);
const input=JSON.parse(inputJson);
const load=name=>import(pathToFileURL(`${root}/packages/orchestrator/src/${name}.ts`).href);
if(mode==="storage"){
 const results=[];
 for(const [name,operation] of [["read-machine-dir","loadMachine"],["read-instances-dir","loadInstances"],["parent-file","saveMachine"],["mkdir-blocked","saveMachine"],["delete-directory","deleteMachine"],["read-permission","loadMachine"],["write-permission","saveMachine"],["delete-permission","deleteMachine"],["write-full","saveMachine"]]){
  const dir=`${input.dir}/${name}`;
  if(name==="parent-file")writeFileSync(dir,"block");
  else if(name==="mkdir-blocked"){writeFileSync(dir,"block");}
  else if(name.endsWith("permission")){
   mkdirSync(dir,{recursive:true});writeFileSync(`${dir}/machine.json`,"{}");
   if(name==="delete-permission")chmodSync(dir,0o555);else chmodSync(`${dir}/machine.json`,name==="read-permission"?0:0o444);
  }else if(name==="write-full"){
   if(!existsSync("/dev/full"))throw new Error("sandbox requires /dev/full for write failure fixture");
   mkdirSync(dir,{recursive:true});symlinkSync("/dev/full",`${dir}/machine.json`);
  }else{mkdirSync(dir,{recursive:true});mkdirSync(`${dir}/${name.includes("instances")?"instances.json":"machine.json"}`);}
  const configured=name==="mkdir-blocked"?`${dir}/nested`:dir;
  process.env.PI_ORCHESTRATOR_DIR=configured;
  const storage=await load("storage");
  try{storage[operation]({id:"m",createdAt:"time"});throw new Error("operation unexpectedly succeeded");}
  catch(error){results.push({operation,dir:configured,message:error.message});}
 }
 process.stdout.write(`${JSON.stringify(results)}\n`);
}else if(mode==="spawn"){
 const {RpcProcessInstance}=await load("rpc-process");
 RpcProcessInstance.prototype.getSpawnCommand=()=>({command:input.program,args:input.args??[]});
 let rpc;
 try{rpc=new RpcProcessInstance({cwd:input.cwd});await rpc.send({type:"get_state",id:"probe"});throw new Error("spawn unexpectedly succeeded");}
 catch(error){process.stdout.write(`${JSON.stringify({message:error.message})}\n`);}
 if(rpc)await rpc.dispose();
}else{
 const {RpcProcessInstance}=await load("rpc-process");
 RpcProcessInstance.prototype.getSpawnCommand=()=>({command:process.execPath,args:["-e",input.script]});
 const rpc=new RpcProcessInstance({cwd:input.cwd});
 // Observe callback error without letting Node terminate this diagnostic-only probe.
 rpc.process.stdin.on("error",()=>{});
 await new Promise(resolve=>rpc.onEvent(resolve));
 try{await rpc.send({type:"get_state",id:"probe"});throw new Error("stdin unexpectedly writable");}
 catch(error){process.stdout.write(`${JSON.stringify({message:error.message})}\n`);}
 await rpc.dispose();
}
