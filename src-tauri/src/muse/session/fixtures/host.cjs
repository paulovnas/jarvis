const readline = require('node:readline');
const fs = require('node:fs');
const mode = process.argv[1];
const sid = '01a14128-b34c-7281-b020-39f7fdc25e02';
let tid;
const send = x => process.stdout.write(JSON.stringify({jsonrpc:'2.0',...x})+'\n');
const event = (method,params) => send({method,params:{sessionId:sid,viewCursor:'fixture',...params}});
const marker = stage => { if(process.env.JARVIS_MSP_SESSION_MARKER) fs.appendFileSync(process.env.JARVIS_MSP_SESSION_MARKER,stage+'\n'); };
readline.createInterface({input:process.stdin}).on('line',line=>{
 const r=JSON.parse(line);
 if(r.method==='initialize') send({id:r.id,result:{schema:{version:1,fingerprint:'sha256:5d3b43e5ccdb7dc4f5a32c380f5f2cde0f10e3059b93610afb2e73f73b1d914c'},serverInfo:{name:'muse',version:'fixture'},experimentalApi:true,grantedCapabilities:mode!=='noSessionMcp'&&r.params.capabilities?.requestedCapabilities?.includes('sessionMcp')?['sessionMcp']:[],sessionDurability:'ephemeral'}});
 if(r.method==='account/read') send({id:r.id,result:{state:mode==='apiKey'?'apiKey':'accountLogin',label:'session-fixture'}});
 if(r.method==='usage/read') send({id:r.id,result:{}});
 if(r.method==='model/list') send({id:r.id,result:{providerId:'meta',source:'providerCatalog',models:[{providerId:'meta',modelId:'muse-fixture',variants:['low','high'],defaultReasoningEffort:'low'}]}});
 if(r.method==='session/start') {
  marker('session/start');
  if(r.params.providerId!=='meta'||r.params.modelId!=='muse-fixture'||r.params.workspaceRoot!==process.cwd()) return send({id:r.id,error:{code:-32602,message:'invalid fixture session'}});
  send({id:r.id,result:{viewCursor:'fixture',session:{sessionId:sid,workspaceRoot:process.cwd(),providerId:'meta',modelId:'muse-fixture',path:'',status:'idle'}}});
  event('session/modelChanged',{modelId:mode==='changedModel'?'another-model':'muse-fixture',providerId:mode==='changedProvider'?'local':'meta'});
 }
 if(r.method==='turn/start') {
  marker('turn/start');tid=r.params.commandId;
  if(!r.params.input?.length||r.params.input[0].type!=='text'||r.params.reasoningEffort!=='low') return send({id:r.id,error:{code:-32602,message:'invalid fixture turn'}});
  if(process.env.JARVIS_MSP_EXPECT_LANGUAGE && !r.params.input[0].text.includes(process.env.JARVIS_MSP_EXPECT_LANGUAGE)) return send({id:r.id,error:{code:-32602,message:'missing response language'}});
  send({id:r.id,result:{commandId:tid,status:'accepted',turnId:tid,startedNewTurn:true,disposition:'started'}});
  send({method:'usage/changed',params:{usage:{observedAtMs:Date.now(),tier:'Pro',window:{usedPercent:25,windowDurationMins:300,resetsAtMs:Date.now()+3600000},weekly:{usedPercent:40,resetsAtMs:Date.now()+86400000}}}});
  if(mode==='hold') return;
  event('item/completed',{item:{itemId:'answer',kind:'agentMessage',turnId:tid,status:'completed',text:'Fixture verified answer.',messagePhase:'finalAnswer'}});
  if(mode!=='noSearch') event('item/completed',{item:{itemId:'search',kind:'webSearch',turnId:tid,status:'completed',action:'search',providerStatus:'completed',query:'official source',results:[{title:'Official source',url:'https://example.org/official'}]}});
  event('turn/completed',{turnId:tid,terminal:mode==='failed'?'failed':'completed',error:{message:'private-secret-must-not-echo'}});
 }
 if(r.method==='turn/interrupt') {marker('turn/interrupt');send({id:r.id,result:{commandId:r.params.commandId,status:'accepted'}});}
});
