// Offline pinned bridge/cc-session-io oracle. Args: convert.ts, cc-session-io dist/index.js.
import { pathToFileURL } from 'node:url';
import crypto from 'node:crypto';
import { syncBuiltinESMExports } from 'node:module';
let sequence = 0;
crypto.randomUUID = () => `${(++sequence).toString(16).padStart(8,'0')}-1111-4111-8111-111111111111`;
syncBuiltinESMExports();
const convert = await import(pathToFileURL(process.argv[2]));
const io = await import(pathToFileURL(process.argv[3]));
const use = id => ({type:'tool_use',id,name:'read',input:{path:'fixture.txt'}});
const result = id => ({type:'tool_result',tool_use_id:id,content:'answer',is_error:false});
const assistant = content => ({role:'assistant',content});
const user = content => ({role:'user',content});
const raw = [
  ['orphan-use', [assistant([use('a')])]],
  ['orphan-result', [user([result('orphan')])]],
  ['interleaved-pairs', [assistant([use('a'),use('b')]),user([result('a'),{type:'text',text:'steer'}]),user([result('b')])]],
  ['pair-and-orphan', [assistant([use('a')]),user([result('orphan'),result('a')]),user('next')]],
  ['utf16-and-duplicates', [assistant([use('\ud800'),use('\ud800')]),user([result('\ud800'),result('\ud800')])]],
];
const usage = {input:0,output:0,cacheRead:0,cacheWrite:0,totalTokens:0,cost:{input:0,output:0,cacheRead:0,cacheWrite:0,total:0}};
const piAssistant = content => ({role:'assistant',content,api:'test',provider:'other',model:'m',usage,stopReason:'stop',timestamp:1});
const piUse = id => ({type:'toolCall',id,name:'read',arguments:{path:'fixture.txt'}});
const piResult = id => ({role:'toolResult',toolCallId:id,toolName:'read',content:[{type:'text',text:'answer'}],isError:false,timestamp:2});
const tail = piAssistant([{type:'text',text:'tail'}]);
const imports = [
  ['orphan-use', [piAssistant([piUse('a')]),tail]],
  ['orphan-result', [piResult('orphan'),tail]],
  ['interleaved-pairs', [piAssistant([piUse('a'),piUse('b')]),piResult('a'),{role:'user',content:'steer',timestamp:3},piAssistant([]),piResult('b'),tail]],
].map(([name,input]) => {
  const converted = convert.convertPiMessages(input,new Map()).anthropicMessages;
  const session = new io.Session({sessionId:'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',projectPath:'/fixture/project',jsonlPath:'/unused',cwd:'/fixture/project',model:'m'});
  session.importMessages(converted);
  return {name,input,converted,repaired:io.repairToolPairing(converted),messages:session.records.map(record => record.message)};
});
console.log(JSON.stringify({source:'38d84cb549cce66dd76d45a5020d132f74780393',dependency:'cc-session-io@0.4.0',cases:raw.map(([name,input])=>({name,input,expected:io.repairToolPairing(input)})),imports}));
