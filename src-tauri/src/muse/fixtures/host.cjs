const fs = require('node:fs');
const readline = require('node:readline');
const mode = process.argv[1];
const directory = process.argv[2];
const fixture = name => JSON.parse(fs.readFileSync(`${directory}/${name}.json`, 'utf8'));
const methods = [];
let catalogStale = false;
const send = value => process.stdout.write(`${JSON.stringify({jsonrpc:'2.0', ...value})}\n`);
readline.createInterface({input:process.stdin}).on('line', line => {
  const frame = JSON.parse(line);
  if (!frame.method) return;
  methods.push(frame.method);
  if (!('id' in frame)) return;
  const reply = result => send({id:frame.id,result});
  if (frame.method === 'initialize') {
    if (frame.params?.clientInfo?.name !== 'jarvis' || frame.params?.capabilities?.experimentalApi !== true) process.exit(10);
    return reply({experimentalApi:mode !== 'unsupported',grantedCapabilities:[],museHome:'/fixture/.muse',platformFamily:'unix',platformOs:'linux',schema:{version:1,fingerprint:`sha256:${'a'.repeat(64)}`},serverInfo:{name:'muse-session-server',version:'fixture-1'},userAgent:'fixture'});
  }
  if (frame.method === 'account/read') {
    const account = fixture('account');
    if (mode === 'apiKey' || mode === 'loggedOut') account.state = mode;
    if (mode === 'notificationUsageOnly') send({method:'usage/changed',params:fixture('usage').usage});
    return reply(account);
  }
  if (frame.method === 'model/list' && mode !== 'modelsUnavailable') {
    const catalog = fixture('models');
    if (mode === 'staleCatalogUntilRestart' && catalogStale) catalog.models = [];
    return reply(catalog);
  }
  if (frame.method === 'usage/read' && mode === 'usageUnavailable') return send({id:frame.id,error:{code:-32601,message:'private-account-data-must-not-escape',data:{kind:'methodNotFound',retryable:false}}});
  if (frame.method === 'usage/read') return reply(mode === 'usageEmpty' || mode === 'notificationUsageOnly' ? {} : fixture('usage'));
  if (frame.method === 'account/loginStart') {
    if (frame.params?.type !== 'deviceCode' || frame.params?.apiKey) process.exit(11);
    catalogStale = true;
    return reply({verificationUrl:'https://auth.meta.com/device',userCode:'TEST-CODE'});
  }
  if (frame.method === 'account/loginCancel') return reply({cancelled:true});
  if (frame.method === 'fixture/methods') return reply({methods});
  if (frame.method === 'fixture/environment') return reply({config:process.env.XDG_CONFIG_HOME,data:process.env.XDG_DATA_HOME,state:process.env.XDG_STATE_HOME,auth:process.env.MUSE_AUTH_PATH,home:process.env.HOME,hasApiKey:'META_API_KEY' in process.env});
  if (frame.method === 'fixture/loginCompleted') {
    send({method:'account/loginCompleted',params:{outcome:frame.params.outcome,message:'private-account-data-must-not-escape'}});
    return reply({});
  }
  send({id:frame.id,error:{code:-32601,message:'private-account-data-must-not-escape',data:{kind:'methodNotFound',retryable:false}}});
});
