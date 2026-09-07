# Publicação e atualizações do Jarvis

O Jarvis recebe versões assinadas pelos [Releases do GitHub](https://github.com/paulovnas/jarvis/releases). Instalações beta aceitam prévias e versões estáveis; instalações estáveis recebem apenas versões estáveis. Rascunhos e releases sem os arquivos compatíveis com a arquitetura são ignorados.

## Publicar com um comando

Na branch `main`, com o código commitado e sincronizado, execute:

```sh
bun run release 0.8.4-beta
```

O comando funciona no **macOS, Windows e Linux** com Bun, Git e GitHub CLI autenticado (`gh auth login`). A máquina que inicia o release não precisa de Rust, Xcode, certificado Apple ou da chave privada do atualizador. Isso permite iniciar uma publicação macOS mesmo trabalhando no Windows; o build Windows ainda não está habilitado.

O launcher atualiza a versão em package.json, Tauri e Cargo, cria um commit e uma tag anotada, envia os dois atomicamente e dispara o workflow **Release macOS**. O terminal mostra o link para acompanhar o CI; o retorno do comando confirma o agendamento, e a conclusão do workflow confirma a publicação.

```sh
# Simular sem alterar arquivos nem acessar o GitHub.
bun run release 0.8.4-beta --dry-run

# Usar notas próprias em pt-BR, preservadas na anotação da tag.
bun run release 0.8.4-beta --notes-file /caminho/notas-da-versao.md

# Acompanhar as execuções e depois uma execução específica.
gh run list --workflow release-macos.yml
gh run watch ID_DA_EXECUCAO --exit-status
```

Sem notas próprias, o GitHub gera as notas pelo histórico. Use sempre uma versão SemVer sem o prefixo `v` no comando; por exemplo, `0.8.4-beta` para beta e `0.8.4` para estável. O padrão das próximas prévias é `-beta`, sem sufixo numérico: para publicar outra, avance a versão, como `0.8.5-beta`. A release `0.8.3-beta.1` permanece com seu nome original.

## O que executa no CI

O workflow usa um runner macOS hospedado pelo GitHub e gera o instalador **Apple Silicon (`aarch64-apple-darwin`)**, a mesma arquitetura das primeiras releases. Intel, Universal e Windows ficam para uma ampliação futura, com validação própria.

As etapas são:

1. Validar que a execução veio da `main` do repositório oficial, que a tag aponta para um commit da `main` e que as versões dos quatro arquivos coincidem.
2. Instalar dependências com lockfile e executar lint, typecheck, testes, build, Clippy e testes Rust.
3. Importar o certificado existente em um Keychain temporário, compilar e assinar o app, o DMG e o pacote de atualização.
4. Verificar a assinatura Apple e a assinatura criptográfica do pacote contra a chave pública instalada no Jarvis.
5. Guardar os artefatos por sete dias e remover as chaves temporárias.
6. Em um job separado, conferir novamente o commit e a assinatura, carregar os cinco arquivos em um rascunho e publicar apenas após confirmar o upload completo.

Somente o job de publicação recebe permissão de escrita no conteúdo do repositório. Os secrets pertencem ao ambiente `macos-release`, restrito à branch `main`. O workflow é iniciado manualmente ou pelo launcher; não executa código de pull requests. As actions são fixadas por SHA.

## Validar sem publicar uma versão

Em **Actions → Release macOS → Run workflow**, mantenha a branch `main`, a tag vazia e `publish` desmarcado. Ou execute:

```sh
gh workflow run release-macos.yml --ref main -f publish=false
```

Isso executa os mesmos gates e gera os mesmos instaladores assinados, disponíveis nos artefatos da execução. Não cria tags, não modifica uma release existente e não oferece uma atualização aos usuários.

## Configuração inicial das assinaturas

A configuração foi migrada para GitHub Actions. Para reinstalar ou atualizar os secrets no Mac que possui a identidade Apple original:

```sh
bun run release:setup
```

Esse comando usa a chave local em `~/.jarvis/release/updater.key` e confere seu `.pub` contra `plugins.updater.pubkey`. Opcionalmente, `TAURI_SIGNING_PRIVATE_KEY` pode indicar outro **caminho de arquivo**, com o `.pub` ao lado. Se a chave tiver senha, informe `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` no ambiente.

O setup exporta exclusivamente a identidade Apple selecionada para um PKCS#12 protegido por senha aleatória, envia os valores por stdin ao GitHub CLI e remove os arquivos temporários. O macOS pode solicitar autorização do Keychain. Se houver múltiplas identidades, defina `APPLE_SIGNING_IDENTITY` com o hash escolhido. A configuração recusa ambientes que permitam outras branches e preserva regras de proteção existentes.

Secrets no ambiente **macos-release**:

| Secret | Conteúdo |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Conteúdo da chave privada do atualizador, preservada entre versões. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Senha da chave, quando houver; ausente para a chave atual sem senha. |
| `APPLE_CERTIFICATE` | Identidade Apple com chave privada, em PKCS#12 codificado em base64. |
| `APPLE_CERTIFICATE_PASSWORD` | Senha aleatória do PKCS#12. |
| `APPLE_SIGNING_IDENTITY` | Fingerprint SHA-1 do certificado selecionado. |

Os arquivos locais originais continuam disponíveis como backup. Secrets do GitHub não podem ser baixados depois: mantenha uma cópia segura da chave do atualizador e da identidade Apple fora do Git. Perder ou substituir a chave do atualizador impede que instalações existentes confiem nas novas atualizações.

A assinatura do atualizador e a assinatura Apple são diferentes. O CI preserva ambas, mas **ainda não realiza notarização Apple**. A migração para CI não elimina os avisos de Gatekeeper: isso exige Developer ID e credenciais de notarização, a serem configurados separadamente.

## Falhas, retomada e retirada de uma versão

Um gate que falha impede a publicação. Não é necessário recompilar localmente: consulte o log no Actions. Para uma falha temporária, use **Re-run all jobs** na execução original. Se o agendamento falhar depois do push, o mesmo comando pode ser repetido enquanto a tag ainda aponta para o HEAD. Se a main já avançou, use **Run workflow** na main, informe a tag original e marque `publish`.

Um rascunho incompleto pode ser retomado; versões publicadas e tags existentes nunca são substituídas pelo launcher. Correções de código devem receber uma nova versão. O CI verifica novamente a identidade do commit antes de publicar.

Para retirar uma atualização problemática da descoberta, converta sua release em rascunho:

```sh
gh release edit vVERSAO --draft
```

Isso impede novas instalações automáticas dessa release, mas não reverte aplicativos já atualizados. Publique a correção com versão maior; o atualizador não faz downgrade.

## Como a atualização funciona

O texto verde **Atualização Disponível** abre as notas da versão. **Atualizar e reiniciar** baixa o pacote com indicação de progresso, verifica a assinatura, instala e reabre o Jarvis. Execuções de agentes, compactações manuais, processos persistentes e instalações do Core precisam terminar antes. Novas execuções e instalações do Core ficam bloqueadas durante a atualização. As preferências de layout são salvas antes da substituição e da reabertura.

No macOS, o novo processo confirma a abertura da janela usando a versão esperada e um segredo temporário enviado por uma conexão local. O processo anterior só encerra após essa confirmação. Se a abertura falhar ou exceder o prazo, a janela antiga permanece disponível e o botão **Reabrir Jarvis** tenta novamente sem repetir o download. Essa confirmação verifica a criação da janela nativa, não o funcionamento de todos os recursos da aplicação.

É necessário executar uma cópia instalada do `.app` com permissão de escrita. Instale a primeira versão manualmente: copie o aplicativo do DMG para Aplicativos e abra essa cópia. Versões antigas sem atualizador não conseguem fazer essa migração sozinhas, e uma cópia executada diretamente do DMG não pode ser atualizada no local.

## Arquivos publicados e validação

Cada release inclui o DMG, o `.app.tar.gz`, a assinatura `.sig`, um `latest.json` padrão e o manifesto `latest-darwin-aarch64.json` e/ou `latest-darwin-x86_64.json`. Os manifestos contêm a URL do pacote assinado, versão, data de publicação e notas. O atualizador aceita apenas arquivos da tag `v<versão>` correspondente neste repositório.

O comando de publicação não abre o aplicativo nem substitui uma instalação em execução. Os testes automatizados cobrem seleção de versão e canal, geração de manifestos, interface de progresso e erros, exclusão mútua com execuções ativas e protocolo de confirmação da nova janela. A validação completa exige testar download, instalação e reabertura entre duas versões publicadas no Mac de destino.
