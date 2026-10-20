async function main() {
 process.stdin.setEncoding("utf8");
 let pending = "";
 for await (const chunk of process.stdin) {
  pending += chunk;
  let end: number;
  while ((end = pending.indexOf("\n")) !== -1) {
   const request = JSON.parse(pending.slice(0, end)); pending = pending.slice(end + 1);
   console.log(JSON.stringify({locale: Intl.DateTimeFormat().resolvedOptions().locale,
    dates: request.millis.map((ms: number) => new Date(ms).toLocaleString())}));
  }
 }
 if (pending.length) throw new Error("Unterminated oracle request");
}
main().catch(error => { console.error(error); process.exitCode = 1; });
