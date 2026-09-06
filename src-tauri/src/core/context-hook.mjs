// Internal Jarvis adapter. This entry point never installs hooks in other hosts.
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { SessionDB, resolveSessionDbPath } from './node_modules/context-mode/hooks/session-db.bundle.mjs';
import { extractEvents } from './node_modules/context-mode/hooks/session-extract.bundle.mjs';
import { buildResumeSnapshot } from './node_modules/context-mode/hooks/session-snapshot.bundle.mjs';

let raw = '';
for await (const chunk of process.stdin) {
  raw += chunk;
  if (Buffer.byteLength(raw) > 1024 * 1024) throw new Error('Hook input too large');
}
const input = JSON.parse(raw);
const sessionsDir = join(process.env.CONTEXT_MODE_DIR, 'sessions');
mkdirSync(sessionsDir, { recursive: true });
const db = new SessionDB({ dbPath: resolveSessionDbPath({ projectDir: input.cwd, sessionsDir }) });
let context = '';
try {
  db.ensureSession(input.session_id, input.cwd);
  const save = (type, category, data, priority = 2) => db.insertEvent(input.session_id, { type, category, data, priority }, input.event);
  switch (input.event) {
    case 'session_start':
      context = db.getResume(input.session_id)?.snapshot ?? '';
      break;
    case 'user_prompt':
      save('user_prompt', 'user-prompt', input.text.slice(0, 16000), 1);
      break;
    case 'post_tool': {
      const names = { bash: 'Bash', read: 'Read', write: 'Write', edit: 'Edit', list: 'Glob', search: 'Grep' };
      const toolInput = { ...input.args, file_path: input.args?.path };
      for (const event of extractEvents({ tool_name: names[input.name] ?? input.name, tool_input: toolInput, tool_response: input.output, tool_output: input.failed ? { isError: true } : undefined })) {
        db.insertEvent(input.session_id, event, 'PostToolUse');
      }
      if (input.failed) save('tool_error', 'error', `${input.name}: ${input.output.slice(0, 4000)}`, 1);
      break;
    }
    case 'pre_compact': {
      const events = db.getEvents(input.session_id);
      context = buildResumeSnapshot(events);
      db.upsertResume(input.session_id, context, events.length);
      break;
    }
    case 'post_compact':
      db.incrementCompactCount(input.session_id);
      save('compaction_summary', 'compaction', input.text.slice(0, 16000), 1);
      break;
    case 'turn_end':
      save('assistant_response', 'decision', input.text.slice(0, 8000));
      break;
    default: throw new Error('Unknown internal hook');
  }
  process.stdout.write(JSON.stringify({ context: context.slice(0, 16000) }));
} finally { db.close(); }
