// Keep this file valid browser JavaScript: Rust embeds it verbatim in the
// standalone OAuth callback page, which does not load the Vite application.
(() => {
  const closeButton = document.getElementById("close-tab");
  const feedback = document.getElementById("close-feedback");
  if (!(closeButton instanceof HTMLButtonElement) || !(feedback instanceof HTMLElement)) return;

  closeButton.addEventListener("click", () => {
    closeButton.disabled = true;
    closeButton.setAttribute("aria-busy", "true");

    // close() returns no result and may only log a browser-policy warning.
    // A surviving page must explain the blocked action instead of staying silent.
    window.setTimeout(() => {
      if (window.closed) return;
      const shortcut = /Mac|iPhone|iPad|iPod/i.test(navigator.platform) ? "⌘ + W" : "Ctrl + W";
      feedback.textContent = `Seu navegador bloqueou o fechamento pelo botão. Use ${shortcut} ou o X da aba e volte ao Jarvis.`;
      feedback.hidden = false;
      closeButton.disabled = false;
      closeButton.removeAttribute("aria-busy");
      feedback.focus();
    }, 300);

    try {
      window.close();
    } catch {
      // Some browser hosts throw instead; the same fallback remains available.
    }
  });
})();
