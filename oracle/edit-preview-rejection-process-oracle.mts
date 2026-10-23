import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

const source=process.argv[2];
const path=process.env.PI_EDIT_PREVIEW_REJECTION_PATH;
if(!source || !path) throw new Error("preview rejection process requires source and path");
const module=await import(pathToFileURL(source).href);
const theme=await import(pathToFileURL(join(dirname(source),"../../modes/interactive/theme/theme.ts")).href);
theme.initTheme("dark");
const args={path,edits:[{oldText:"before",newText:"after"}]};
try {
    module.createEditToolDefinition("/repo").renderCall(args,theme.theme,{
        args,toolCallId:"preview-rejection",invalidate:()=>{},lastComponent:undefined,state:{},cwd:"/repo",
        executionStarted:false,argsComplete:true,isPartial:false,expanded:false,showImages:false,isError:false,
    });
} catch { /* Catch only the synchronous renderer error; leave its Promise unhandled. */ }
console.log("SYNC_TURN_END");
setTimeout(()=>console.log("NO_REJECTION"),700);
