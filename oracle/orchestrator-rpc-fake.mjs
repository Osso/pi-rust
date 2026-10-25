import readline from "node:readline";
for await (const line of readline.createInterface({input:process.stdin})) {
 const command=JSON.parse(line);
 if(command.type === "exit") process.exit(7);
 if(command.type === "sigkill") {process.kill(process.pid,"SIGKILL");continue;}
 if(command.type === "hang") continue;
 if (command.type === "hold") {
  process.stdout.write(JSON.stringify({type:"received",id:command.id})+"\n");
  continue;
 }
 if(command.type === "unterminated") {process.stdout.write(JSON.stringify({type:"response",success:true,id:command.id,command:command.type}));process.exit(0);}
 if(command.type === "invalid_utf8") {process.stdout.write(Buffer.concat([Buffer.from(`{"type":"response","success":true,"id":"${command.id}","command":"invalid_utf8","data":{"label":"`),Buffer.from([255]),Buffer.from('"}}\n')]));process.exit(0);}
 if(command.type === "stderr_utf8") {const bytes=Buffer.from("中");process.stderr.write(bytes.subarray(0,1));await new Promise(resolve=>setTimeout(resolve,20));process.stderr.write(bytes.subarray(1));process.exit(7);}
 if(command.type === "malformed") {process.stdout.write("{\n");continue;}
 if(command.type === "extension_ui_response") continue;
 process.stdout.write(JSON.stringify({type:"turn_end",id:command.id})+"\n");
 process.stdout.write(JSON.stringify({type:"extension_ui_request",id:"ui",method:"confirm",title:"Test"})+"\n");
 process.stdout.write(JSON.stringify({type:"response",id:command.id,command:command.type,success:true,data:{sessionId:"session",sessionFile:"/tmp/fake.jsonl"}})+"\n");
}
