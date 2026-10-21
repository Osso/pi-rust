import readline from "node:readline";
for await (const line of readline.createInterface({input:process.stdin})) {
 const command=JSON.parse(line);
 if(command.type === "exit") process.exit(7);
 if(command.type === "hang") continue;
 if(command.type === "malformed") {process.stdout.write("{\n");continue;}
 if(command.type === "extension_ui_response") continue;
 process.stdout.write(JSON.stringify({type:"turn_end",id:command.id})+"\n");
 process.stdout.write(JSON.stringify({type:"extension_ui_request",id:"ui",method:"confirm",title:"Test"})+"\n");
 process.stdout.write(JSON.stringify({type:"response",id:command.id,command:command.type,success:true,data:{sessionId:"session",sessionFile:"/tmp/fake.jsonl"}})+"\n");
}
