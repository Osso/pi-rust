import {createInterface} from "node:readline";
import {pathToFileURL} from "node:url";
const source=process.argv[2]!;
const {parseOsc11BackgroundColor}=await import(pathToFileURL(`${source}/tui/src/terminal-colors.ts`).href);
const {detectTerminalBackgroundTheme}=await import(pathToFileURL(`${source}/coding-agent/src/modes/interactive/theme/theme.ts`).href);
for await(const line of createInterface({input:process.stdin})){
 const request=JSON.parse(line);
 const output=[];
 for(const value of request.cases){
  const rgb=value.report?parseOsc11BackgroundColor(value.report):Object.fromEntries(Object.entries(value.channels).map(([key,value])=>[key,Number(value)]));
  output.push(await detectTerminalBackgroundTheme({ui:{queryTerminalBackgroundColor:async()=>rgb},timeoutMs:100,env:{}}));
 }
 console.log(JSON.stringify({id:request.id,ok:true,output}));
}
