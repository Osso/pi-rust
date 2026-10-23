import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
const api = await import(pathToFileURL(process.argv[2]).href);
for await (const line of createInterface({ input: process.stdin })) {
    const request = JSON.parse(line);
    const output = [];
    for (const input of request.paths) {
        try { output.push({value: api.normalizePath(input)}); }
        catch (error) {
            const value = {name:error.name,message:error.message};
            for (const key of Object.getOwnPropertyNames(error)) {
                if (key !== "stack" && key !== "name" && key !== "message") value[key] = error[key];
            }
            output.push({error:value,constructor:error.constructor.name,text:String(error)});
        }
    }
    console.log(JSON.stringify(output));
}
