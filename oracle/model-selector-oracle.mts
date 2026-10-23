import {createInterface} from "node:readline";
import {writeFileSync} from "node:fs";
import {pathToFileURL} from "node:url";
const root=process.argv[2]!;
const {ModelSelectorComponent}=await import(pathToFileURL(`${root}/modes/interactive/components/model-selector.ts`).href);
const {ModelRegistry}=await import(pathToFileURL(`${root}/core/model-registry.ts`).href);
const {AuthStorage}=await import(pathToFileURL(`${root}/core/auth-storage.ts`).href);
const themes=await import(pathToFileURL(`${root}/modes/interactive/theme/theme.ts`).href);
const {Text}=await import(pathToFileURL(`${root}/../../tui/src/index.ts`).href);
const units=(value:string)=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
for await(const line of createInterface({input:process.stdin})){
 const request=JSON.parse(line);
 try{
  themes.setThemeInstance(new themes.Theme(Object.fromEntries(request.colors.map((key:string)=>[key,0])),{},'256color'));
  writeFileSync(request.path,request.initial);
  const registry=new ModelRegistry(AuthStorage.inMemory(),request.path);
  const find=(item:any)=>registry.find(item.provider,item.id)??item;
  const current=request.current?find(request.current):undefined;
  const scoped=request.scoped.map((model:any)=>({model:find(model),thinkingLevel:'high'}));
  writeFileSync(request.path,request.refreshed);
  const events:any[]=[];
  let renders=0;
  const selector=new ModelSelectorComponent({requestRender:()=>renders++},current,registry,scoped,(model:any)=>events.push({provider:model.provider,id:model.id,name:units(model.name)}),()=>events.push('cancel'),request.query??undefined);
  let width=request.width;
  const added:any[]=[];
  const snapshot=()=>({lines:selector.render(width).map(units),events:[...events],renders,query:units(selector.getSearchInput().getValue()),focused:selector.getSearchInput().focused,children:selector.children.length});
  const frames=[snapshot()];
  for(const step of request.steps){
   const value=step.units?String.fromCharCode(...step.units):step.value;
   switch(step.op){
    case 'settle':await new Promise(resolve=>setImmediate(resolve));break;
    case 'key':selector.handleInput(value);break;
    case 'inputKey':selector.getSearchInput().handleInput(value);break;
    case 'setInput':selector.getSearchInput().setValue(value);break;
    case 'focus':selector.focused=step.value;break;
    case 'width':width=step.value;break;
    case 'invalidate':selector.invalidate();break;
    case 'add':{const child=new Text(step.value,0,0);added.push(child);selector.addChild(child);break;}
    case 'remove':selector.removeChild(added[step.index]);break;
    case 'clear':selector.clear();break;
    default:throw new Error(`unknown step ${step.op}`);
   }
   frames.push(snapshot());
  }
  console.log(JSON.stringify({id:request.id,ok:true,output:frames}));
 }catch(error){console.log(JSON.stringify({id:request.id,ok:false,error:String(error)}));}
}
