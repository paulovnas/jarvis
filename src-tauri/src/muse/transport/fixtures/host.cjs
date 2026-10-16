// Deterministic wire host; envelopes follow the pinned MSP stable schema.
const readline = require('node:readline');
const lines = readline.createInterface({ input: process.stdin });
const send = frame => process.stdout.write(JSON.stringify({jsonrpc: '2.0', ...frame}) + (mode === 'crlf' ? '\r\n' : '\n'));
const mode = process.argv[1];
const golden = mode === 'official' ? process.argv[2].trim().split('\n').map(line => JSON.parse(line)) : undefined;
if (mode === 'crlf') process.stdout.write('\n \r\n');
let initialization;
let initialized = false;
const reverse = [];
const replies = [];
let holds = 0;
lines.on('line', line => {
  const frame = JSON.parse(line);
  if (frame.method === 'initialize') {
    initialization = frame.params;
    if (golden) { process.stdout.write(golden[1].raw + '\n'); return; }
    send({id: frame.id, result: {
      experimentalApi: frame.params.capabilities?.experimentalApi === true,
      grantedCapabilities: frame.params.capabilities?.requestedCapabilities || [],
      museHome: '/fixture/.muse', platformFamily: 'unix', platformOs: 'linux',
      schema: {version: mode === 'version' ? 99 : 1, fingerprint: 'sha256:c8d1a2a1866814e220fd396d382a9a75861412feee884b5021b2ee359bd3dc59'},
      serverInfo: {name: 'muse-session-server', version: '0.0.0-fixture'},
      userAgent: 'muse-session-server/fixture', sessionDurability: 'durable'
    }});
    return;
  }
  if (frame.method === 'initialized') {
    initialized = true;
    send({method: 'fixture/initialized', params: {initialization}});
    if (mode === 'blocked') { lines.close(); process.stdin.pause(); setInterval(() => {}, 1000); }
    return;
  }
  if (!initialized) { process.exit(89); }
  if (!frame.method) {
    replies.push({id: frame.id, result: frame.result, error: frame.error});
    if (replies.length === 2) send({method: 'fixture/replies', params: {replies}});
    return;
  }
  if (frame.method === 'fixture/echo') send({id: frame.id, result: {params: frame.params}});
  if (frame.method === 'fixture/hold') holds++;
  if (frame.method === 'fixture/count') send({id: frame.id, result: {holds}});
  if (frame.method === 'fixture/pending') send({method: 'fixture/holding', params: {id: frame.id}});
  if (frame.method === 'fixture/reverse') {
    reverse.push(frame);
    if (reverse.length === 2) for (const request of reverse.reverse()) send({id: request.id, result: request.params});
  }
  if (frame.method === 'fixture/wrongType') {
    send({id: String(frame.id), result: {wrong: true}});
    send({id: frame.id, result: {correct: true}});
  }
  if (frame.method === 'fixture/error') send({id: frame.id, error: {code: -32601, message: 'private-secret-do-not-echo', data: {kind: 'methodNotFound', retryable: false}}});
  if (frame.method === 'fixture/malformed') process.stdout.write('private-secret-do-not-echo\n');
  if (frame.method === 'fixture/oversize') process.stdout.write('x'.repeat(10_485_761));
  if (frame.method === 'fixture/invalid') send({id: frame.id, result: {}, error: {code: -1, message: 'bad', data: {kind: 'internal'}}});
  if (frame.method === 'fixture/partial') { process.stdout.write('{"jsonrpc":"2.0","id":1,"result":{}}'); process.exit(23); }
  if (frame.method === 'fixture/eof') { process.stderr.write('private-secret-do-not-echo'); process.exit(23); }
  if (frame.method === 'fixture/peer') {
    send({id: 9, method: 'fixture/question', params: {number: true}});
    send({id: '9', method: 'fixture/question', params: {number: false}});
    send({id: frame.id, result: {sent: true}});
  }
  if (frame.method === 'fixture/burst') for (let i = 0; i < 100; i++) send({method: 'fixture/event', params: {i}});
  if (frame.method === 'fixture/tree') {
    const child = require('node:child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio: 'ignore'});
    child.once('spawn', () => send({id: frame.id, result: {parent: process.pid, child: child.pid}}));
  }
});
