import type {
    AssistantImages, AssistantMessage, AssistantMessageEvent, Context, ImagesModel, Model,
    ProviderNativeContext, ProviderResponse, ProviderRetryEvent, TextSignatureV1, ToolResultMessage,
    UserMessage, Usage,
} from "@pi-oracle-source/ai/src/types.ts";
import { createInterface } from "node:readline";

const usage = { input:1, output:2, cacheRead:3, cacheWrite:4, cacheWrite1h:2, reasoning:1, totalTokens:10,
    cost:{input:0.1,output:0.2,cacheRead:0.3,cacheWrite:0.4,total:1} } satisfies Usage;
const native = {provider:"custom",api:"openai-responses",format:"openai.responses.input",value:{"\ud800":[1,true,null]}} satisfies ProviderNativeContext;
const user = {role:"user",content:[{type:"text",text:"hello\ud800"},{type:"image",data:"eA==",mimeType:"image/png"}],inputSource:"extension",providerNative:native,timestamp:1.25} satisfies UserMessage;
const assistant = {role:"assistant",content:[{type:"text",text:"text\ud800",textSignature:"signature"},{type:"thinking",thinking:"think\udc00",thinkingSignature:"opaque",redacted:false},{type:"toolCall",id:"call",name:"run",arguments:{"\ud800":"\udc00"},execution:{agentId:"agent"},thoughtSignature:"thought"}],imageGenerationResult:{type:"image",data:"eA==",mimeType:"image/png"},api:"custom",provider:"custom",model:"model",responseModel:"actual",responseId:"response",usage,stopReason:"toolUse",errorMessage:"error\ud800",timestamp:2.5} satisfies AssistantMessage;
const toolResult = {role:"toolResult",toolCallId:"call",toolName:"run",content:[{type:"text",text:"result\udc00"}],details:{"\ud800":[1,false]},isError:true,timestamp:3.75} satisfies ToolResultMessage;
const model = {id:"model",samplingParams:{"\ud800":"\udc00"},name:"Name\ud800",api:"openai-completions",provider:"custom",baseUrl:"https://example.test",reasoning:true,thinkingLevelMap:{off:null,high:"high"},input:["text","image"],cost:{input:1,output:2,cacheRead:3,cacheWrite:4},contextWindow:4096,autoCompactionThreshold:3000,maxTokens:1024,headers:{"x-test":"value"},compat:{thinkingFormat:"chat-template",chatTemplateKwargs:{enabled:{$var:"thinking.enabled",omitWhenOff:true},temperature:0.5},openRouterRouting:{sort:{by:"price",partition:null},max_price:{prompt:"0.5"},preferred_min_throughput:{p50:10}},vercelGatewayRouting:{only:["anthropic"]}}} satisfies Model<"openai-completions"> & Record<string,unknown>;
const imageModel = {id:"image",custom:{"\ud800":"\udc00"},samplingParams:null,name:"Image\ud800",api:"custom",provider:"custom",baseUrl:"https://example.test",input:["text"],output:["image"],cost:{input:0,output:0,cacheRead:0,cacheWrite:0}} satisfies ImagesModel<"custom"> & Record<string,unknown>;
const images = {api:"custom",provider:"custom",model:"image",output:[{type:"text",text:"caption\ud800"},{type:"image",data:"eA==",mimeType:"image/png"}],responseId:"image-response",usage,stopReason:"aborted",errorMessage:"abort\udc00",timestamp:4.5} satisfies AssistantImages;
const context = {systemPrompt:"system\ud800",messages:[user,assistant,toolResult],tools:[{name:"run",description:"Description\udc00",parameters:{type:"object",properties:{value:{type:"string"}},required:["value"]}}]} satisfies Context;
const response = {status:200,headers:{"x-second":"2","x-first":"1"}} satisfies ProviderResponse;
const retry = {attempt:1,maxAttempts:3,delayMs:1250.5,reason:"retry\ud800",fallbackTransport:"websocket-cached"} satisfies ProviderRetryEvent;
const signature = {v:1,id:"opaque",phase:"final_answer"} satisfies TextSignatureV1;
const events = [
    {type:"start",partial:assistant},
    {type:"text_start",contentIndex:0,partial:assistant},
    {type:"text_delta",contentIndex:0,delta:"x\ud800",partial:assistant},
    {type:"text_end",contentIndex:0,content:"text\ud800",partial:assistant},
    {type:"thinking_start",contentIndex:1,partial:assistant},
    {type:"thinking_delta",contentIndex:1,delta:"y\udc00",partial:assistant},
    {type:"thinking_end",contentIndex:1,content:"think\udc00",partial:assistant},
    {type:"toolcall_start",contentIndex:2,partial:assistant},
    {type:"toolcall_delta",contentIndex:2,delta:"{}",partial:assistant},
    {type:"toolcall_end",contentIndex:2,toolCall:assistant.content.find((block) => block.type === "toolCall")!,partial:assistant},
    {type:"done",reason:"toolUse",message:assistant},
    {type:"error",reason:"aborted",error:assistant},
] satisfies AssistantMessageEvent[];
for await (const _line of createInterface({input:process.stdin})) {
    process.stdout.write(`${JSON.stringify({usage,native,user,assistant,toolResult,model,imageModel,images,context,response,retry,signature,events})}\n`);
}
