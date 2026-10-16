const readline = require('node:readline');
const http = require('node:http');
const mode = process.argv[1];
const session = '01a14128-b34c-7281-b020-39f7fdc25e02';
const turn = '01a14128-b4f0-7a4c-b599-adbf43e8c467';
let mcp;
let sessionMcp = false;
function send(value) { process.stdout.write(JSON.stringify(value) + '\n'); }
function emit(method, params) { send({jsonrpc:'2.0', method, params:{sessionId:session, viewCursor:'opaque-fixture', ...params}}); }
readline.createInterface({input:process.stdin}).on('line', async line => {
  const request = JSON.parse(line);
  const reply = result => send({jsonrpc:'2.0',id:request.id,result});
  if (request.method === 'initialize') {
    sessionMcp = request.params.capabilities?.requestedCapabilities?.includes('sessionMcp');
    return reply({serverInfo:{name:'muse',version:'1.4.9'},schema:{version:1,fingerprint:'sha256:'+'a'.repeat(64)},grantedCapabilities:sessionMcp?['sessionMcp']:[],experimentalApi:true});
  }
  if (request.method === 'initialized') return;
  if (request.method === 'session/start') {
    if (!sessionMcp) return send({jsonrpc:'2.0',id:request.id,error:{code:-32010,message:'sessionMcp required',data:{kind:'capabilityRequired'}}});
    mcp = request.params.config.mcpServers.jarvis;
    return reply({session:{sessionId:session,modelId:'muse-fixture',workspaceRoot:request.params.workspaceRoot},viewCursor:'opaque-fixture'});
  }
  if (request.method === 'turn/start') {
    reply({commandId:request.params.commandId,status:'accepted',turnId:turn,startedNewTurn:true,disposition:'started'});
    send({jsonrpc:'2.0',method:'usage/changed',params:{usage:{observedAtMs:Date.now(),tier:'Pro',window:{usedPercent:25,windowDurationMins:300,resetsAtMs:Date.now()+3600000},weekly:{usedPercent:40,resetsAtMs:Date.now()+86400000}}}});
    if (mode === 'changedModel') emit('session/modelChanged',{providerId:'meta',modelId:'another-model'});
    const actions = mode === 'writeAndBash' ? [
      {name:'update_tasks', arguments:{tasks:[{id:'implementation',title:'Write and verify the fixture file',status:'in_progress'}]}},
      {name:'read', arguments:{path:'proof.txt'}},
      {name:'write', arguments:{path:'proof.txt',content:'updated-proof'}},
      {name:'bash', arguments:{command:'node -p "require(\'node:fs\').readFileSync(\'proof.txt\', \'utf8\')"',yieldTimeMs:1000}},
      {name:'update_tasks', arguments:{tasks:[{id:'implementation',title:'Write and verify the fixture file',status:'completed'}]}},
    ] : Array.from({length:mode === 'sequential' ? 2 : 1}, () => ({name:'read',arguments:{path:'proof.txt'}}));
    for (const [index, action] of actions.entries()) {
    emit('item/started',{item:{itemId:'tool-item-'+index,kind:'toolCall',callId:'call-1',tool:'mcp__jarvis__'+action.name,args:JSON.stringify(action.arguments)}});
    const body = JSON.stringify({jsonrpc:'2.0',id:index+1,method:'tools/call',params:action});
    const target = new URL(mcp.url);
    const response = await new Promise((resolve,reject) => {
      const req = http.request(target, {method:'POST',headers:{...mcp.headers,'Content-Type':'application/json','Content-Length':Buffer.byteLength(body)}}, res => {
        let text='';res.on('data',d => text+=d);res.on('end',()=>resolve(JSON.parse(text)));
      });
      req.on('error',reject);req.end(body);
    });
    if (response.result?.isError) throw new Error('Jarvis rejected the authorized action: '+JSON.stringify(response));
    if (action.name === 'read' && !JSON.stringify(response).includes('fixture-proof')) throw new Error('Jarvis did not return the confirmed file result');
    if (action.name === 'bash' && !JSON.stringify(response).includes('updated-proof')) throw new Error('Command did not verify the actual written file: '+JSON.stringify(response));
    if (mode === 'disconnect') return process.exit(1);
    emit('item/completed',{item:{itemId:'tool-item-'+index,kind:'toolCall',callId:'call-1',tool:'mcp__jarvis__'+action.name,args:JSON.stringify(action.arguments),status:'completed'}});
    }
    emit('item/completed',{item:{itemId:'answer',kind:'agentMessage',text:'Arquivo lido.',messagePhase:'finalAnswer'}});
    emit('session/tokenUsage',{turnId:turn,modelId:'muse-fixture',usage:{inputTokens:9,outputTokens:7,cachedTokens:0,reasoningTokens:0},promptTokens:9,totalTokens:16,cumulative:{promptTokens:9,outputTokens:7,totalTokens:16},durationMs:875});
    emit('session/tokenUsage',{turnId:turn,modelId:'muse-fixture',usage:{inputTokens:9,outputTokens:7,cachedTokens:0,reasoningTokens:0},promptTokens:9,totalTokens:16,cumulative:{promptTokens:9,outputTokens:7,totalTokens:16},durationMs:875});
    emit('turn/completed',{turnId:turn,terminal:'completed',usage:{inputTokens:9,outputTokens:7,cachedTokens:0,reasoningTokens:0}});
    return;
  }
  if (request.method === 'turn/interrupt') return reply({});
  if (request.id !== undefined) send({jsonrpc:'2.0',id:request.id,error:{code:-32601,message:'Unsupported method'}});
});
