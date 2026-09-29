import { BrowserController } from "./browser";
import { errorResult, object, pairingSchema, requestSchema, type Pairing } from "./protocol";

type Status = { state: "disconnected" | "connecting" | "connected" | "error"; message: string };
let pairing: Pairing | undefined;
let socket: WebSocket | undefined;
let controller: BrowserController;
let instanceId = "";
let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
let heartbeat: ReturnType<typeof setInterval> | undefined;
let handshakeTimer: ReturnType<typeof setTimeout> | undefined;
let attempt = 0;
let lastMessage = 0;
let status: Status = { state: "disconnected", message: "Cole o código de conexão fornecido pelo Jarvis." };
const seen = new Set<string>();
const pending = new Map<string, { cancelled: boolean }>();

function setStatus(value: Status) {
  status = value;
  void chrome.storage.session.set({ connectionStatus: value });
  void chrome.action.setBadgeText({ text: value.state === "connected" ? "ON" : "" });
  void chrome.action.setBadgeBackgroundColor({ color: "#397650" });
}

function send(value: unknown, connection = socket) {
  if (connection?.readyState === WebSocket.OPEN) connection.send(JSON.stringify(value));
}

function scheduleReconnect() {
  if (!pairing || reconnectTimer) return;
  // A bounded backoff and alarm survive short network failures and worker sleep;
  // only the transport reconnects. Requests are never replayed.
  reconnectTimer = setTimeout(() => { reconnectTimer = undefined; connect(); }, Math.min(1000 * 2 ** Math.min(attempt++, 4), 10000));
}

function disconnect() {
  clearTimeout(reconnectTimer); reconnectTimer = undefined;
  clearTimeout(handshakeTimer); clearInterval(heartbeat);
  const previous = socket;
  socket = undefined;
  previous?.close();
}

function connect() {
  if (!pairing || socket && socket.readyState < WebSocket.CLOSING) return;
  setStatus({ state: "connecting", message: "Conectando ao Jarvis…" });
  const config = pairing;
  const connection = new WebSocket(config.endpoint);
  socket = connection;
  let ready = false;
  handshakeTimer = setTimeout(() => connection.close(), 8000);
  connection.onopen = () => {
    send({ type: "hello", version: 1, token: config.token, instanceId, epoch: controller.epoch,
      label: "Navegador Chromium", extensionVersion: chrome.runtime.getManifest().version }, connection);
  };
  connection.onmessage = event => {
    if (connection !== socket || typeof event.data !== "string" || event.data.length > 256000) return;
    let message: Record<string, unknown>;
    try { message = object(JSON.parse(event.data)); } catch { return; }
    lastMessage = Date.now();
    if (message.type === "ready" && message.version === 1) {
      if (ready) return;
      ready = true;
      attempt = 0;
      clearTimeout(handshakeTimer);
      setStatus({ state: "connected", message: "Conectado. As abas só serão controladas quando solicitadas no Jarvis." });
      heartbeat = setInterval(() => {
        if (Date.now() - lastMessage > 65000) connection.close();
        else send({ type: "ping" }, connection);
      }, 20000);
      return;
    }
    if (!ready) return;
    if (message.type === "ping") { send({ type: "pong" }, connection); return; }
    if (message.type === "cancel" && typeof message.id === "string") {
      const operation = pending.get(message.id);
      if (operation) operation.cancelled = true;
      return;
    }
    if (message.type !== "request") return;
    const parsed = requestSchema.safeParse(message);
    if (!parsed.success) {
      if (typeof message.id === "string") send({ type: "result", id: message.id, ok: false, error: { code: "browser_invalid_request", message: "A operação recebida não corresponde ao protocolo da extensão." } }, connection);
      return;
    }
    const request = parsed.data;
    if (seen.has(request.id)) {
      send({ type: "result", id: request.id, ok: false, error: { code: "browser_outcome_unknown", message: "Esta operação já foi recebida. Inspecione a aba; uma ação não será repetida automaticamente." } }, connection);
      return;
    }
    seen.add(request.id);
    if (seen.size > 500) seen.delete(seen.values().next().value!);
    const operation = { cancelled: false };
    pending.set(request.id, operation);
    void controller.execute(request, () => !operation.cancelled && connection === socket && connection.readyState === WebSocket.OPEN).then(
      result => send({ type: "result", id: request.id, ok: true, result }, connection),
      error => send({ type: "result", id: request.id, ok: false, error: errorResult(error) }, connection),
    ).finally(() => pending.delete(request.id));
  };
  connection.onerror = () => connection.close();
  connection.onclose = () => {
    if (connection !== socket) return;
    socket = undefined;
    clearTimeout(handshakeTimer); clearInterval(heartbeat);
    setStatus({ state: "error", message: ready ? "A conexão foi interrompida. Tentando reconectar, sem repetir ações." : "Abra o Jarvis e confira o código de conexão. Tentando novamente…" });
    scheduleReconnect();
  };
}

const initialized = (async () => {
  await chrome.storage.local.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
  await chrome.storage.session.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
  const local = await chrome.storage.local.get(["pairing", "instanceId"]);
  const session = await chrome.storage.session.get("epoch");
  instanceId = typeof local.instanceId === "string" ? local.instanceId : crypto.randomUUID();
  const epoch = typeof session.epoch === "string" ? session.epoch : crypto.randomUUID();
  await chrome.storage.local.set({ instanceId });
  await chrome.storage.session.set({ epoch });
  controller = new BrowserController(epoch, conversationId => send({ type: "changed", conversationId }));
  await controller.restore();
  const config = pairingSchema.safeParse(local.pairing);
  pairing = config.success ? config.data : undefined;
  await chrome.alarms.create("jarvis-reconnect", { periodInMinutes: 0.5 });
  connect();
})().catch(() => {
  setStatus({ state: "error", message: "Não foi possível inicializar a conexão. Recarregue a extensão e tente novamente." });
});

chrome.action.onClicked.addListener(() => { void chrome.runtime.openOptionsPage(); });
chrome.alarms.onAlarm.addListener(alarm => {
  if (alarm.name === "jarvis-reconnect") void initialized.then(connect);
});
chrome.tabs.onRemoved.addListener(id => { void initialized.then(() => controller?.removed(id)); });
chrome.tabs.onUpdated.addListener((id, info) => { void initialized.then(() => controller?.updated(id, info)); });
chrome.debugger.onDetach.addListener(source => { if (source.tabId !== undefined) void initialized.then(() => controller?.onDetach(source.tabId!)); });
chrome.debugger.onEvent.addListener((source, method, params) => { if (source.tabId !== undefined) void initialized.then(() => controller?.event(source.tabId!, method, params)); });
chrome.runtime.onMessage.addListener((raw: unknown, sender, respond: (value: unknown) => void) => {
  if (sender.id !== chrome.runtime.id) return false;
  const message = object(raw);
  if (!["connect", "disconnect", "status"].includes(String(message.type))) return false;
  void initialized.then(async () => {
    if (!controller) throw new Error("Recarregue a extensão antes de conectar.");
    if (message.type === "connect") {
      const config = pairingSchema.safeParse(message.pairing);
      if (!config.success) throw new Error("Código inválido. Copie o código completo nas configurações do Jarvis.");
      disconnect();
      await controller.releaseAll();
      pairing = config.data;
      await chrome.storage.local.set({ pairing });
      attempt = 0;
      connect();
    }
    if (message.type === "disconnect") {
      pairing = undefined;
      await chrome.storage.local.remove("pairing");
      disconnect();
      await controller.releaseAll();
      setStatus({ state: "disconnected", message: "Desconectado. As abas foram preservadas." });
    }
    return { ok: true, status };
  }).then(respond, error => respond({ ok: false, error: error instanceof Error ? error.message : "Falha ao configurar a conexão." }));
  return true;
});
