/** Jarvis-owned projection of native execution into Impeccable's existing chrome. */
(function () {
  'use strict';
  if (window.__JARVIS_IMPECCABLE_LIVE__) return;
  let current = null;
  let chrome = null;
  let panel = null;
  let observer = null;
  let pending = null;
  let feedback = null;
  let sequence = 0;
  let completed = false;
  const requests = [];
  const indicators = new Map();
  const prefix = 'jarvis-impeccable-live';
  const phases = {
    connecting: 'Conectando ao Designer…',
    waiting: 'Aguardando o agente ficar disponível…',
    running: 'Designer trabalhando',
    blocked: 'A alteração precisa de atenção',
    idle: 'Live pronto para editar',
  };
  function text(node, value) {
    if (node.textContent !== value) node.textContent = value;
  }
  function element(tag, label) {
    const node = document.createElement(tag);
    if (label) node.setAttribute('aria-label', label);
    return node;
  }
  function duration(value) {
    const seconds = Math.floor(Math.max(0, value || 0) / 1000);
    return seconds < 60 ? seconds + ' s' : Math.floor(seconds / 60) + ' min ' + seconds % 60 + ' s';
  }
  function request(action) {
    if (!current || pending || requests.length) return;
    if (action === 'cancel' && !['waiting', 'running'].includes(current.phase)) return;
    if (action === 'resume' && current.phase !== 'blocked' && current.state !== 'setup' && !current.error) return;
    pending = current.conversationId + ':' + (crypto.randomUUID ? crypto.randomUUID() : Date.now() + '-' + (++sequence));
    feedback = null;
    requests.push({ requestId: pending, action, conversationId: current.conversationId, tabId: current.tabId, actionId: current.actionId, revision: current.revision });
    render();
  }
  function mount() {
    const next = window.__IMPECCABLE_LIVE_CHROME_CORE__;
    const bar = next?.getById?.('impeccable-live-global-bar');
    if (!bar || !bar.isConnected) return false;
    if (panel?.isConnected && chrome === next) return true;
    chrome = next;
    panel?.remove();
    panel = element('div', 'Execução do Designer no Jarvis');
    panel.id = prefix + '-status';
    panel.setAttribute('role', 'status');
    panel.setAttribute('aria-live', 'polite');
    panel.style.cssText = 'display:flex;align-items:center;gap:8px;min-width:0;max-width:340px;padding:6px 10px;border-right:1px solid currentColor;font:12px system-ui;color:inherit;';
    panel.style.color = next.getById('impeccable-live-pick-toggle')?.style.color || next.getById('impeccable-live-global-bar-brand')?.style.color || 'inherit';
    const copy = element('div');
    copy.style.cssText = 'min-width:0;display:grid;gap:4px;line-height:1.3;';
    const title = element('strong'); title.id = prefix + '-title';
    const detail = element('span'); detail.id = prefix + '-detail';
    detail.style.cssText = 'font:11px ui-monospace,monospace;opacity:.8;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
    const message = element('span'); message.id = prefix + '-message';
    message.style.cssText = 'font-size:11px;overflow-wrap:anywhere;';
    copy.append(title, detail, message);
    const actions = element('div'); actions.style.cssText = 'display:flex;gap:5px;flex-shrink:0;';
    for (const [action, label] of [['cancel', 'Cancelar alteração'], ['resume', 'Retomar']]) {
      // Compose with the installed toolbar's button chrome rather than a parallel UI.
      const existing = next.getById('impeccable-live-page-chat-send');
      const button = existing ? existing.cloneNode(false) : element('button');
      button.removeAttribute('id'); button.removeAttribute('title');
      button.type = 'button'; button.id = prefix + '-' + action;
      button.setAttribute('aria-label', label); text(button, label);
      button.style.cssText += ';width:auto;height:auto;min-width:0;padding:6px 8px;border-radius:5px;font:11px system-ui;cursor:pointer;';
      button.addEventListener('click', event => { event.stopPropagation(); request(action); });
      actions.append(button);
    }
    panel.append(copy, actions);
    bar.insertBefore(panel, bar.firstChild);
    const brand = next.getById('impeccable-live-global-bar-brand');
    if (brand && !brand.dataset.jarvisStatus) {
      brand.dataset.jarvisStatus = 'true';
      // The upstream tooltip guesses busy from its animation, not the native turn.
      brand.addEventListener('mouseenter', event => event.stopImmediatePropagation(), true);
    }
    return true;
  }
  function render() {
    if (!current || current.state === 'off' || !mount()) { panel?.remove(); return; }
    const find = id => panel.querySelector('#' + prefix + '-' + id);
    const blocked = current.phase === 'blocked' || current.state === 'setup' || !!current.error;
    const phase = current.state === 'starting' ? 'connecting' : blocked ? 'blocked' : current.phase;
    const shader = chrome.getById('impeccable-live-shader');
    if (shader && phase !== 'running') {
      if (!indicators.has(shader)) indicators.set(shader, { visibility: shader.style.visibility, priority: shader.style.getPropertyPriority('visibility') });
      shader.style.setProperty('visibility', 'hidden', 'important');
    }
    if (phase === 'running') {
      for (const [node, previous] of indicators) { node.style.setProperty('visibility', previous.visibility, previous.priority); }
      indicators.clear();
    }
    const title = pending ? 'Enviando ação ao Jarvis…' : completed && phase === 'idle' ? 'Alteração concluída. Revise o resultado.' : phases[phase] || phases.idle;
    text(find('title'), title);
    const details = [current.provider, current.model, current.step ? 'Etapa ' + current.step : '', current.elapsedMs ? duration(current.elapsedMs) : '', current.queueCount ? current.queueCount + ' na fila' : ''].filter(Boolean).join(' · ');
    text(find('detail'), details);
    find('detail').title = details;
    const message = feedback || current.error || current.activity || '';
    text(find('message'), message);
    find('message').hidden = !message;
    const cancel = find('cancel'); cancel.hidden = !current.actionId || !['waiting', 'running'].includes(phase); cancel.disabled = !!pending;
    const resume = find('resume'); resume.hidden = !blocked; resume.disabled = !!pending;
    panel.dataset.phase = phase;
    const brand = chrome.getById('impeccable-live-global-bar-brand');
    if (brand) { brand.title = title; brand.setAttribute('aria-label', title); }
  }
  function update(value) {
    if (!value || typeof value.conversationId !== 'string' || typeof value.tabId !== 'string' || !Number.isSafeInteger(value.revision)) return;
    if (current && (value.conversationId !== current.conversationId || value.tabId !== current.tabId || value.revision < current.revision)) return;
    if (current?.actionId && !value.actionId && value.phase === 'idle' && !value.error) completed = true;
    if (value.actionId && value.actionId !== current?.actionId) completed = false;
    if (current && value.revision > current.revision && !value.error && value.phase !== 'blocked' && value.state !== 'setup') feedback = null;
    current = value;
    window.__JARVIS_IMPECCABLE_SESSION__?.(value);
    if (value.feedback?.requestId === pending) { pending = null; feedback = value.feedback.error || null; }
    render();
  }
  function dispose() {
    observer?.disconnect(); observer = null;
    panel?.remove(); panel = null;
    requests.length = 0; pending = null; current = null;
    for (const [node, previous] of indicators) node.style.setProperty('visibility', previous.visibility, previous.priority);
    indicators.clear();
  }
  // HMR can remount the upstream root; observe only structure, never page text.
  observer = new MutationObserver(() => { if (current && !panel?.isConnected) render(); });
  observer.observe(document.documentElement, { childList: true, subtree: true });
  Object.defineProperty(window, '__JARVIS_IMPECCABLE_LIVE__', {
    configurable: false, writable: false,
    value: { version: 1, update, drain: () => requests.splice(0, 1), dispose },
  });
})();
