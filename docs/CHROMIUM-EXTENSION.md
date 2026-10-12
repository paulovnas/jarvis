# Integrated Chromium and the optional browser extension

Jarvis includes Chromium Embedded Framework (CEF) in the application bundle. Its runtime and helper processes update with Jarvis; they are not a separately installed Core component. The application requires macOS 13 or later on Apple Silicon, matching the embedded Chromium framework. Windows users do not need WebView2 or a separately installed Chrome browser.

New browser tabs open inside the chat. Their session is separate from the user's personal browser, and Impeccable Live runs only in the integrated tab. Connecting an external extension does not redirect navigation or add Live controls to external pages. **Configurações → Navegador** has no global browser-mode selector; its browser selection applies only to extension installation.

## Install the optional external extension

Use the extension when you want the agent to inspect an existing page in your personal browser. It supports Chrome, Edge, Brave, Chromium and Firefox.

1. Open **Configurações → Navegador** and choose **Navegador da extensão**.
2. Select **Preparar extensão**. Use **Abrir pasta** to locate the prepared files.
3. Load the extension in your browser:
   - Chrome, Edge, Brave or Chromium: open the extensions address shown by Jarvis, enable developer mode, choose **Carregar sem compactação**, and select the prepared directory.
   - Firefox: open `about:debugging#/runtime/this-firefox`, choose **Carregar extensão temporária**, and select the prepared `manifest.json`. Grant site access if requested. Temporary installation must be repeated after restarting Firefox; permanent installation requires a Mozilla-signed package.
4. Open the Jarvis extension's options, paste the code from **Copiar código de conexão**, and connect. Keep the code private.

Keep the browser open for automatic reconnection. One browser profile can be paired at a time. To change the paired browser, select **Revogar conexão** and prepare a new code. After updating Jarvis, prepare the files again and reload the extension.

## Inspect an external tab

Choose an external tab for the chat or explicitly ask the agent to inspect it. Discovery lists available tabs without adopting them; attachment binds the chosen tab to one conversation. The connection provides page snapshots, screenshots, console messages, network summaries and scoped DevTools where supported. Console and network capture starts at attachment and is not retroactive.

External inspection does not replace integrated navigation. To edit a page with Impeccable Live, open it in the integrated chat browser and enable Live there.

## Check the bundled runtime

**Configurações → Ferramentas → Core** and **Diagnóstico e Reparo** show the included Chromium runtime separately from optional video-rendering runtimes. If the application bundle is incomplete, reinstall or update Jarvis; there is no separate Chromium download or reinstall button in Core.
