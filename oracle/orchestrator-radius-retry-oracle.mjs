import {pathToFileURL} from "node:url";
const [root, scenario] = process.argv.slice(2);
process.env.PI_RADIUS_API_KEY="key";
const NativeDate=Date;
globalThis.Date=class extends NativeDate {constructor(...args){super(...(args.length?args:["2026-10-21T00:00:00.000Z"]));}};
Math.random=()=>0.5;
const schedules=[];
globalThis.setTimeout=(_callback,delay)=>{schedules.push(delay);return schedules.length;};
globalThis.clearTimeout=()=>{};
const registration=(id,interval)=>({id,heartbeatIntervalMs:interval,expiresInMs:interval*2});
const sequence=scenario==="machine"
 ? [[200,registration("m1",10)],[500,"oops"],[500,"oops"],[200,{}],[404,"gone"],[404,"gone"],[404,"gone"],[200,registration("m2",10)],[200,registration("p1",10)],[200,{}]]
 : [[200,registration("m1",1000000)],[200,registration("p1",10)],[500,"oops"],[200,{}],[404,"gone"],[404,"gone"],[404,"gone"],[200,registration("p2",10)],[200,{}],[200,{}]];
const requests=[],updates=[];
globalThis.fetch=async(url,init)=>{
 const body=JSON.parse(init.body);for(const key of ["hostname","platform","arch","pid"])delete body[key];
 requests.push({url:String(url),body});const [status,data]=sequence.shift();return new Response(typeof data==="string"?data:JSON.stringify(data),{status});
};
const {RadiusPresence}=await import(pathToFileURL(`${root}/packages/orchestrator/src/radius.ts`).href);
const radius=new RadiusPresence();
let instance={id:"local",cwd:"/tmp",sessionId:"session"};
radius.setCoordinator({getLiveInstance:()=>instance,listLiveInstances:()=>[instance],updateInstance:value=>{instance=value;updates.push(value);}});
await radius.start("desk");
if(scenario==="machine"){
 for(let i=0;i<6;i++)await radius.heartbeatMachine();
}else{
 instance=await radius.registerPi(instance);for(let i=0;i<5;i++)await radius.heartbeatPi(instance.id);await radius.disconnectPi(instance);
}
await radius.stop();
process.stdout.write(`${JSON.stringify({requests,updates,schedules})}\n`);
