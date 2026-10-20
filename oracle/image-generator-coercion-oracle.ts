import { createInterface } from "node:readline";
async function main() {
  for await (const line of createInterface({input: process.stdin})) {
    const request = JSON.parse(line);
    try {
      if (request.operation === "compare") {
        process.stdout.write(JSON.stringify({order: Math.sign(JSON.parse(request.left).localeCompare(JSON.parse(request.right)))}) + "\n");
      } else {
        const value = JSON.parse(request.value);
        process.stdout.write(JSON.stringify({truthy:Boolean(value), text:JSON.stringify(String(value))}) + "\n");
      }
    } catch (error) { process.stdout.write(JSON.stringify({name:error.name,message:error.message}) + "\n"); }
  }
}
void main();
