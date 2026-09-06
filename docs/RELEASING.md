# Publicação e atualizações do Jarvis

A versão atual é `0.8.1-beta.1`, exibida como **0.8.1 Beta**. O aplicativo consulta os [Releases do GitHub](https://github.com/paulovnas/jarvis/releases) após iniciar, a cada seis horas e ao retornar à janela quando a última consulta estiver vencida. O botão da versão no rodapé também permite verificar manualmente. No macOS, **Jarvis → Sobre o Jarvis** abre a mesma modal.

Instalações beta aceitam novas prévias e versões estáveis. Instalações estáveis recebem apenas versões estáveis. A comparação usa SemVer e exige um manifesto compatível com a arquitetura do computador. Rascunhos e versões sem os arquivos necessários são ignorados. Falhas de consulta permitem nova tentativa e não são tratadas como confirmação de que o aplicativo está atualizado.

## Publicar pelo terminal

Execute no macOS, na branch `main`, com as alterações já commitadas e sincronizadas com o GitHub. É necessário ter Bun, Rust, autenticação no GitHub CLI (`gh auth login`) e a identidade Apple de assinatura disponível.

```sh
# Conferir o plano sem alterar arquivos, compilar ou publicar.
bun run release 0.8.1-beta.2 --dry-run

# Publicar a próxima beta com notas próprias em pt-BR.
bun run release 0.8.1-beta.2 --notes-file /caminho/notas-da-versao.md

# Publicar a versão estável, quando estiver pronta.
bun run release 0.8.1
```

O comando sincroniza a versão em `package.json`, Tauri e Cargo; executa lint, checagem de tipos, testes e compilação da interface, além de Clippy e testes Rust; gera e assina o aplicativo, o DMG e o pacote de atualização; cria o commit da versão e a tag; envia ambos ao GitHub; anexa os arquivos a um release em rascunho e só então o publica. Sem `--notes-file`, o GitHub gera as notas automaticamente a partir do histórico.

Qualquer falha interrompe o processo. Revise as alterações locais de versão antes de tentar novamente. Versões publicadas não são sobrescritas; um rascunho correspondente à mesma tag pode ser retomado.

O empacotamento usa o modo CI do Tauri para dispensar a decoração da janela do Finder via AppleScript. O DMG continua contendo o aplicativo assinado e o atalho para Aplicativos.

## Arquiteturas

O padrão é a arquitetura do Mac usado para compilar. A primeira beta é distribuída para Apple Silicon (`aarch64-apple-darwin`). Para uma versão futura que atenda Apple Silicon e Intel no mesmo pacote, instale os dois alvos Rust e gere um aplicativo universal:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bun run release 0.8.1-beta.2 --target universal-apple-darwin
```

O comando de publicação atualmente gera apenas pacotes macOS. Escolha a opção universal antes de publicar quando precisar atender às duas arquiteturas; o comando não acrescenta uma segunda arquitetura a uma versão já publicada.

## Chaves e assinaturas

A chave privada do atualizador fica **fora do repositório**, em `~/.jarvis/release/updater.key`. O arquivo público correspondente, `updater.key.pub`, deve coincidir com `plugins.updater.pubkey` em `src-tauri/tauri.conf.json`. Guarde uma cópia de segurança dos dois em local seguro. Perder ou substituir essa chave impede que as instalações existentes confiem nas próximas atualizações.

Para usar outro local, defina `TAURI_SIGNING_PRIVATE_KEY` com o **caminho de um arquivo** e mantenha o `.pub` ao lado dele. Caso a chave privada tenha senha, informe-a na variável de ambiente `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` do processo de publicação. Chaves privadas e senhas nunca devem entrar no Git, nos anexos do release ou nas notas. A chave local inicial não tem senha e usa permissões de acesso exclusivas do proprietário.

A assinatura do atualizador e a assinatura Apple têm funções diferentes. O Jarvis verifica a assinatura do pacote antes de instalá-lo. O comando Tauri também mantém a identidade Apple estável usada pelo aplicativo, preservando sua identidade de acesso ao Keychain entre compilações.

Distribuir para outros Macs sem alertas do Gatekeeper exige um certificado **Developer ID** apropriado e credenciais de notarização Apple compatíveis com o Tauri. A primeira beta ainda não é notarizada. A assinatura do atualizador não substitui essa etapa.

Compilações de desenvolvimento comuns não precisam da chave privada do atualizador. A configuração adicional `src-tauri/tauri.release.conf.json` habilita a geração dos pacotes assinados de atualização.

## Como a atualização funciona

O texto verde **Atualização Disponível** abre as notas da versão. **Atualizar e reiniciar** baixa o pacote com indicação de progresso, verifica a assinatura, instala e reabre o Jarvis. Execuções de agentes, compactações manuais, processos persistentes e instalações do Core precisam terminar antes. Novas execuções e instalações do Core ficam bloqueadas durante a atualização. As preferências de layout são salvas antes da substituição e da reabertura.

No macOS, o novo processo confirma a abertura da janela usando a versão esperada e um segredo temporário enviado por uma conexão local. O processo anterior só encerra após essa confirmação. Se a abertura falhar ou exceder o prazo, a janela antiga permanece disponível e o botão **Reabrir Jarvis** tenta novamente sem repetir o download. Essa confirmação verifica a criação da janela nativa, não o funcionamento de todos os recursos da aplicação.

É necessário executar uma cópia instalada do `.app` com permissão de escrita. Instale a primeira versão manualmente: copie o aplicativo do DMG para Aplicativos e abra essa cópia. Versões antigas sem atualizador não conseguem fazer essa migração sozinhas, e uma cópia executada diretamente do DMG não pode ser atualizada no local.

## Arquivos publicados e validação

Cada release inclui o DMG, o `.app.tar.gz`, a assinatura `.sig`, um `latest.json` padrão e o manifesto `latest-darwin-aarch64.json` e/ou `latest-darwin-x86_64.json`. Os manifestos contêm a URL do pacote assinado, versão, data de publicação e notas. O atualizador aceita apenas arquivos da tag `v<versão>` correspondente neste repositório.

O comando de publicação não abre o aplicativo nem substitui uma instalação em execução. Os testes automatizados cobrem seleção de versão e canal, geração de manifestos, interface de progresso e erros, exclusão mútua com execuções ativas e protocolo de confirmação da nova janela. A validação completa exige testar download, instalação e reabertura entre duas versões publicadas no Mac de destino.
