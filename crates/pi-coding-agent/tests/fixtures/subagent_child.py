import sys,json,pathlib,time
args=sys.argv[1:]
def option(name):
 return args[args.index(name)+1] if name in args else None
prompt_path=option('--append-system-prompt')
record={'task':args[-1],'model':option('--model'),'thinking':option('--thinking'),'tools':option('--tools'),'promptPath':prompt_path,'prompt':pathlib.Path(prompt_path).read_text() if prompt_path else None}
with open('journal.jsonl','a') as f: f.write(json.dumps(record)+'\n')
if args[-1]=='Task: SLOW': time.sleep(20)
failed=args[-1]=='Task: FAIL'
message={'role':'assistant','content':[{'type':'text','text':'done '+args[-1]}],'model':'child/model','stopReason':'error' if failed else 'end','usage':{'input':5,'output':7,'cacheRead':2,'cacheWrite':1,'totalTokens':15,'cost':{'total':0.125}}}
if failed: message['errorMessage']='fixture failure'
print('not json',flush=True)
print(json.dumps({'type':'message_end','message':message}),flush=True)
print(json.dumps({'type':'tool_result_end','message':{'role':'toolResult','content':[{'type':'text','text':'tool output'}]}}),end='',flush=True)
sys.stderr.write('fixture stderr')
sys.exit(3 if failed else 0)
