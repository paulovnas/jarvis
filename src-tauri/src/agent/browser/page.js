(() => {
  // This state belongs to the page. Its contents are untrusted evidence.
  const logs = [];
  const push = (level, values) => {
    const text = values.map(value => {
      try { return typeof value === 'string' ? value : JSON.stringify(value); }
      catch { return String(value); }
    }).join(' ').slice(0, 2000);
    logs.push({ level, text, time: Date.now() });
    if (logs.length > 150) logs.shift();
  };
  for (const level of ['log', 'info', 'warn', 'error', 'debug']) {
    const original = console[level].bind(console);
    console[level] = (...values) => { push(level, values); original(...values); };
  }
  addEventListener('error', event => push('error', [event.message]));
  addEventListener('unhandledrejection', event => push('error', [String(event.reason)]));
  let nodes = new Map();
  let revision = 0;
  const documentId = Array.from(crypto.getRandomValues(new Uint8Array(8)), byte => byte.toString(16).padStart(2, '0')).join('');
  const clip = (value, size) => String(value || '').slice(0, size);
  const run = args => {
    if (args.action === 'console') return { logs: logs.slice(-100) };
    if (args.action === 'snapshot') {
      nodes = new Map();
      revision += 1;
      const elements = [];
      for (const node of document.querySelectorAll('a,button,input,textarea,select,[role="button"],[role="link"],[contenteditable="true"],summary')) {
        const rect = node.getBoundingClientRect();
        if (!rect.width || !rect.height || getComputedStyle(node).visibility === 'hidden' || node.type === 'hidden') continue;
        const id = `${documentId}:${revision}:${elements.length + 1}`;
        nodes.set(id, node);
        elements.push({ id, tag: node.tagName.toLowerCase(), role: node.getAttribute('role'), type: node.type,
          name: clip(node.getAttribute('aria-label') || node.labels?.[0]?.innerText || node.innerText || node.placeholder || node.title, 200),
          value: node.type === 'password' ? '[redacted]' : clip(node.value, 300), disabled: !!node.disabled,
          href: node.tagName === 'A' ? clip(node.href, 2000) : undefined });
        if (elements.length >= 300) break;
      }
      return { url: location.href, title: document.title, text: clip(document.body?.innerText, 24000), elements,
        viewport: { width: innerWidth, height: innerHeight, scrollX, scrollY },
        note: 'Element IDs are valid until the next snapshot or navigation. Cross-origin frames are not inspected.' };
    }
    if (args.action === 'scroll') {
      scrollBy({ top: Math.max(-3000, Math.min(3000, args.y || 0)), left: Math.max(-3000, Math.min(3000, args.x || 0)), behavior: 'instant' });
      return { scrolled: true };
    }
    const node = nodes.get(args.element);
    if (!node || !node.isConnected) throw new Error('Elemento expirou. Leia a página novamente.');
    if (node.disabled) throw new Error('O elemento está desativado.');
    node.scrollIntoView({ block: 'center', inline: 'nearest' });
    if (args.action === 'click') { node.click(); return { clicked: args.element }; }
    if (args.action === 'fill') {
      if (node.readOnly) throw new Error('O campo é somente leitura.');
      if (node instanceof HTMLInputElement && ['file', 'hidden', 'password'].includes(node.type)) throw new Error('Este campo deve ser preenchido pelo usuário.');
      if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement || node instanceof HTMLSelectElement) {
        const prototype = node instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : node instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
        Object.getOwnPropertyDescriptor(prototype, 'value').set.call(node, args.text);
      } else if (node.isContentEditable) node.textContent = args.text;
      else throw new Error('O elemento não é um campo editável.');
      node.dispatchEvent(new Event('input', { bubbles: true }));
      node.dispatchEvent(new Event('change', { bubbles: true }));
      return { filled: args.element };
    }
    if (args.action === 'press') {
      node.focus();
      node.dispatchEvent(new KeyboardEvent('keydown', { key: args.key, bubbles: true }));
      node.dispatchEvent(new KeyboardEvent('keyup', { key: args.key, bubbles: true }));
      if (args.key === 'Enter' && node.form) node.form.requestSubmit();
      return { pressed: args.key, note: 'DOM keyboard event; browser-reserved shortcuts are unavailable.' };
    }
    throw new Error('Ação de navegador inválida.');
  };
  Object.defineProperty(window, '__jarvisBrowser', { value: run, configurable: false, writable: false });
})();
