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
}else if(mode==="ipc-reset"){
 process.env.PI_ORCHESTRATOR_DIR=input.dir;
 const {createServer}=await import("node:net");
 const server=createServer({pauseOnConnect:true},socket=>setTimeout(()=>socket.destroy(),20));
 await new Promise(resolve=>server.listen(`${input.dir}/orchestrator.sock`,resolve));
 const {sendIpcRequest}=await load("ipc/client");
 try{await sendIpcRequest({type:"list"});throw new Error("reset request unexpectedly succeeded");}
 catch(error){process.stdout.write(`${JSON.stringify({message:error.message})}\n`);}
 await new Promise(resolve=>server.close(resolve));
}else if(mode==="ipc"){
 process.env.PI_ORCHESTRATOR_DIR=input.dir;
 try{
  if(input.action==="client"){
   const {sendIpcRequest}=await load("ipc/client");await sendIpcRequest({type:"list"});
  }else{
   const {startIpcServer}=await load("ipc/server");
   const handler=request=>request.type==="rpc_stream"?{type:"rpc_ready",ok:true,instance:{id:"i"}}:{type:"list_result",ok:true,instances:[]};
   handler.openRpcStream=(_id,respond)=>({handleRequest:async request=>respond({type:"response",success:true,id:request.id}),close(){}});
   const server=await startIpcServer(handler);
   if(input.action==="stream-close"){
    const {createConnection}=await import("node:net");const {createInterface}=await import("node:readline");
    const socket=createConnection(`${input.dir}/orchestrator.sock`);const lines=createInterface({input:socket})[Symbol.asyncIterator]();
    await new Promise(resolve=>socket.once("connect",resolve));socket.write('{"type":"rpc_stream","instanceId":"i"}\n');await lines.next();
    const closed=new Promise(resolve=>server.once("close",resolve));server.close();
    socket.write('{"type":"get_state","id":"after-close"}\n');const response=JSON.parse((await lines.next()).value);socket.end();await closed;
    process.stdout.write(`${JSON.stringify(response)}\n`);
   }else{
    await new Promise(resolve=>server.close(resolve));process.stdout.write(`${JSON.stringify({existsAfterClose:existsSync(`${input.dir}/orchestrator.sock`)})}\n`);
   }
   process.exit(0);
  }
  throw new Error("operation unexpectedly succeeded");
 }catch(error){process.stdout.write(`${JSON.stringify({message:error.message})}\n`);}
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
