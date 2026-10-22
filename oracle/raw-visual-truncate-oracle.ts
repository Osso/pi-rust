import {createInterface} from "node:readline";
import {pathToFileURL} from "node:url";
import path from "node:path";
async function main():Promise<void> {
 const module=await import(pathToFileURL(path.join(process.argv[2]!,"modes/interactive/components/visual-truncate.ts")).href);
 for await(const line of createInterface({input:process.stdin})) {
  try {
   const request=JSON.parse(line);
   const result=module.truncateToVisualLines(String.fromCharCode(...request.units),request.limit,request.width,request.padding);
   process.stdout.write(JSON.stringify({ok:true,json:JSON.stringify([result.visualLines,result.skippedCount])})+"\n");
  }catch(error){process.stdout.write(JSON.stringify({ok:false,error:String(error)})+"\n");}
 }
}
void main();
