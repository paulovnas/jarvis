import { Dialog as JarvisDialog, DialogContent as JarvisDialogContent } from "@/components/ui/dialog";
import { Button as JarvisButton } from "@/components/ui/button";
import { Input as JarvisInput } from "@/components/ui/input";
import { Checkbox as JarvisCheckbox } from "@/components/ui/checkbox";
import { Textarea as JarvisTextarea } from "@/components/ui/textarea";
import { NativeSelect as JarvisNativeSelect,NativeSelectOption as JarvisNativeSelectOption } from "@/components/ui/native-select";

const jarvisVisibleLabels: Record<string,string> = {"Image Cockpit for Codex Workflows":"Suíte de pixel art","v0.2.1":"Motor Image Cockpit 0.2.1 · MIT","Animation Timeline Review / Edit":"Revisão e edição da linha do tempo","Edit timing and game metadata without regenerating PNG frames.":"Edite o tempo e os metadados do jogo preservando os quadros PNG.","Saved":"Salvo","Unsaved":"Não salvo","Metadata only":"Somente metadados","Direction":"Direção","Play":"Reproduzir","Pause":"Pausar","Speed":"Velocidade","Undo":"Desfazer","Redo":"Refazer","Recipe defaults":"Restaurar receita","Hold +100ms":"Pausar +100 ms","Duplicate":"Duplicar","Reverse":"Inverter","Loop":"Repetição","One shot":"Uma vez","Events":"Eventos","Recipe / User origin":"Origem: receita / usuário","Game coordinates":"Coordenadas no jogo","Selected frame":"Quadro selecionado","Close":"Fechar","Save timeline":"Salvar linha do tempo","Pack v2 Round-trip":"Verificar pacote v2","Loop start":"Início da repetição","Loop seam":"Transição da repetição","Animation Review Cockpit":"Revisão de animações","Candidate compare, QC and adoption":"Comparação, qualidade e adoção dos candidatos","Calibration JSON":"JSON de calibração","Close review":"Fechar revisão","Synchronized playback controls":"Controles de reprodução sincronizada","Previous frame":"Quadro anterior","Next frame":"Próximo quadro","Previous direction":"Direção anterior","Next direction":"Próxima direção","Normalized":"Normalizado","Raw":"Original","Source":"Referência","Autoplay paused by reduced motion":"Reprodução automática pausada pela preferência de movimento reduzido","Compare every candidate at the same direction, frame and speed.":"Compare os candidatos na mesma direção, quadro e velocidade.","Loading candidate artifacts…":"Carregando arquivos dos candidatos…","Failed":"Falhou","Human Review":"Revisão visual","Onion":"Quadros vizinhos","Diff":"Diferenças","Ground":"Chão","Pivot":"Pivô","Socket":"Ponto de encaixe","Hitbox":"Área de impacto","Hurtbox":"Área vulnerável","Background":"Fundo","Checker":"Quadriculado","Light":"Claro","Dark":"Escuro","Winner":"Vencedor","Reject":"Rejeitar","Hold":"Aguardar","Save review":"Salvar revisão","Adopt winner":"Adotar vencedor","Repair direction":"Corrigir direção","VFX Composite Stage":"Composição de efeitos","Close VFX Composite Stage":"Fechar composição","Character animation result":"Animação do personagem","Effect animation result":"Animação do efeito","Open character result":"Abrir personagem","Open effect result":"Abrir efeito","peak sync":"Sincronizar pico","start sync":"Sincronizar início","Safe area":"Área segura","Effect bounds":"Limites do efeito","Clipping guide":"Guia de recorte","No character frame":"Nenhum quadro de personagem","Pause composite":"Pausar composição","Play composite":"Reproduzir composição","Composite frame scrubber":"Posição na composição","Composite direction":"Direção da composição","Legacy Effect Pack / uncalibrated":"Pacote de efeito legado / sem calibração","Reset":"Restaurar","Save setup":"Salvar configuração","Socket / Event":"Ponto de encaixe / evento","Event":"Evento","Effect event sync mode":"Sincronização do efeito com o evento","Basic offset":"Deslocamento","Offset X":"Deslocamento X","Offset Y":"Deslocamento Y","Layer":"Camada","Blend":"Mistura","Front":"Frente","Back":"Atrás","Additive":"Aditiva","Screen":"Clarear","Advanced adjustments":"Ajustes avançados","Scale":"Escala","Rotation":"Rotação","Opacity":"Opacidade","Time scale":"Escala de tempo","Start frame":"Quadro inicial","Peak frame":"Quadro de pico","Export":"Exportar","Import":"Importar","Import composite":"Importar composição","Export layers":"Exportar camadas","Export manifest":"Exportar manifesto","Composite Pack":"Pacote da composição","PNG sheet":"Atlas PNG","Checkerboard":"Transparência","Preview":"Prévia","Overlays":"Sobreposições","Quality":"Qualidade","Notes":"Notas","Advanced diagnostics":"Diagnóstico avançado","Store human judgment as a separate calibration layer.":"Salve a revisão visual como uma avaliação separada das métricas automáticas.","Reject reason tags":"Motivos da rejeição","Review note":"Nota da revisão","Visual judgment, adoption reason, regeneration condition":"Avaliação visual, motivo da adoção e condição para gerar novamente","Saving…":"Salvando…","Save failed":"Falha ao salvar","Starting…":"Iniciando…","Direction × Frame QC Matrix":"Qualidade por direção e quadro","Pass":"Aprovado","Warning":"Aviso","Fail":"Falha","QC heatmap":"Mapa de qualidade","Adjacent diff":"Diferença entre quadros vizinhos","Translate X":"Deslocamento X","Translate Y":"Deslocamento Y","Frame-range Repair":"Corrigir intervalo de quadros","Frame-range repair is disabled because the generator cannot yet preserve the untouched frame range.":"A correção por intervalo está desativada porque o gerador ainda não garante preservar os demais quadros.","Disabled until untouched-frame preservation can be verified.":"Indisponível até ser possível verificar a preservação dos demais quadros.","Preview Studio":"Inspeção visual","Inspect game coordinates and the loop seam together.":"Inspecione as coordenadas no jogo e a transição da repetição.","Prev/next diff":"Diferenças anterior/seguinte","Select an adoption candidate":"Selecione um candidato para adotar","Confirm automatic reasons and warnings before adoption.":"Confira os motivos automáticos e avisos antes de adotar.","Adopting…":"Adotando…","Adopt selected candidate":"Adotar candidato selecionado","Phase 7 / non-destructive layers":"Composição com camadas preservadas","Sync separate character and effect layers by socket and event.":"Sincronize as camadas de personagem e efeito por ponto de encaixe e evento.","Character":"Personagem","Effect":"Efeito","Centroid":"Centro de energia","Palette":"Paleta","non-destructive":"Preserva os arquivos originais","Separate layers remain reconstructable in-engine":"As camadas separadas podem ser reconstruídas no jogo","Layer Sheets":"Atlas das camadas","Manifest JSON":"Manifesto JSON","Reimport Pack":"Reimportar pacote","Combined GIF":"GIF combinado","Combined APNG":"APNG combinado","Composite Sheet":"Atlas combinado","Exporting…":"Exportando…","Composite Pack ZIP":"Pacote combinado ZIP","Pack v2 round-trip: identical":"Pacote v2 exportado e reimportado sem alterações","source":"Referência","Foot drift":"Deslizamento dos pés","BBox":"Limites do desenho","Identity":"Identidade","Static copy":"Cópia estática","Clipping":"Recorte indevido","No artifact":"Nenhum arquivo","Ground line":"Linha do chão","Pivot marker":"Marcador do pivô","Socket marker":"Marcador do ponto de encaixe","Hitbox overlay":"Sobreposição da área de impacto","Hit":"Impacto","Hurtbox overlay":"Sobreposição da área vulnerável","Hurt":"Vulnerável","Candidate A/B/C/D · Sync Compare":"Candidatos A/B/C/D · Comparação sincronizada","Candidate A/B/C · Sync Compare":"Candidatos A/B/C · Comparação sincronizada","Candidate adoption failed.":"Não foi possível adotar o candidato.","Direction Repair could not start.":"Não foi possível iniciar a correção da direção.","directions ·":"direções ·","frames ·":"quadros ·","Repair:":"Correção:","Candidate C:":"Candidato C:","Direction Repair":"Correção de direção","Candidate":"Candidato","Composite setup saved":"Configuração da composição salva","Exporting composite":"Exportando composição","Composite export complete":"Composição exportada","VFX Composite export:":"Exportação da composição de efeitos:","Composite export failed":"Não foi possível exportar a composição","Referenced character or effect result is not available in local history.":"O personagem ou efeito referenciado não está disponível no histórico local.","Composite Pack reimported":"Pacote da composição reimportado","Composite import failed":"Não foi possível importar a composição","VFX Quality v2 · shadow":"Qualidade de efeitos v2 · avaliação auxiliar","Alpha":"Transparência","Normal":"Normal","Anchor":"Âncora","Close timeline editor":"Fechar editor da linha do tempo","Pause animation":"Pausar animação","Play animation":"Reproduzir animação","No frame":"Nenhum quadro","First loop frame":"Primeiro quadro da repetição","Last loop frame":"Último quadro da repetição","One-shot":"Execução única","Ping-pong":"Ida e volta","Animation timeline":"Linha do tempo da animação","Timeline frame":"Quadro da linha do tempo","Frame":"Quadro","+ Pivot":"+ Pivô","+ Anchor":"+ Âncora","+ Socket":"+ Ponto de encaixe","+ Hitbox":"+ Área de impacto","+ Hurtbox":"+ Área vulnerável","No extra metadata on this frame.":"Este quadro não tem metadados adicionais.","Checking…":"Verificando…","Round-trip failed":"Falha na verificação de exportação e reimportação","Pack v2 ZIP":"Pacote v2 ZIP","Could not read the selected animation source for fingerprinting.":"Não foi possível ler a referência selecionada para identificá-la.","Local File":"Arquivo local","Local Generator":"Gerador local","Codex Handoff":"Execução pelo Codex","Local Inbox":"Resultados locais","Use images from this machine":"Usar imagens deste computador","Generate local PNG images":"Gerar imagens PNG localmente","Write local jobs for Codex to pick up":"Preparar operações locais para o Codex","Import results returned by Codex":"Importar resultados produzidos pelo Codex","Settings":"Configurações","Recovery, diagnostics, and environment reporting live here.":"Recuperação, diagnósticos e relatório do ambiente.","General":"Geral","Recovery":"Recuperação","Diagnostics":"Diagnósticos","Environment Report":"Relatório do ambiente","Open Safe Mode":"Abrir modo de recuperação","Open Normal URL":"Abrir endereço normal","Open Reset Local State":"Abrir redefinição do estado local","Diagnose":"Diagnosticar","Recover Results":"Recuperar resultados","Repair Cockpit":"Reparar suíte","Dedupe History":"Remover duplicatas do histórico","Clear History":"Limpar histórico","Clear Frames":"Limpar quadros","Clear Animation Library":"Limpar biblioteca de animações","Clear All Local State":"Limpar todo o estado local","Copy Markdown":"Copiar Markdown","Copy JSON":"Copiar JSON","Refresh":"Atualizar","Image Cockpit":"Suíte de pixel art","ChatGPT image generation availability and local Codex runner imagegen availability are separate states.":"A geração de imagens no ChatGPT e no executor local do Codex têm disponibilidades independentes.","Current state":"Estado atual","Runner:":"Executor:","/ Storage:":"/ Armazenamento:","Codex Runner":"Executor Codex","Storage recovery":"Recuperação do armazenamento","Local State Recovery":"Recuperação do estado local","Origin storage":"Armazenamento deste endereço","Quota:":"Cota:","Startup threshold":"Limites de inicialização","Warning 200 MB / Auto safe 500 MB / Hard block 1 GB":"Aviso em 200 MB / Recuperação automática em 500 MB / Bloqueio em 1 GB","Codex runner":"Executor Codex","Server checks stay available while local state is skipped.":"As verificações do servidor continuam disponíveis quando o estado local não é carregado.","History summary":"Resumo do histórico","Frames summary":"Resumo dos quadros","Reset browser local state":"Redefinir estado local do navegador","These actions never delete repository files, codex-handoff/outbox, docs/qa, or generated PNG files.":"Estas ações preservam os arquivos do repositório, codex-handoff/outbox, docs/qa e os PNGs gerados.","Clear history":"Limpar histórico","Clear frames":"Limpar quadros","Clear animation library":"Limpar biblioteca de animações","Clear all local Image Cockpit state":"Limpar todo o estado local da suíte","Local workspace":"Área de trabalho local","Cockpit:":"Suíte:","/ Runner":"/ Executor","Primary workflows":"Fluxos principais","All":"Todos","Prompt example categories":"Categorias de exemplos de prompt","Search":"Buscar","dash, combat, movement...":"avanço rápido, combate, movimento...","Category":"Categoria","Status":"Estado","Verified":"Verificado","Experimental":"Experimental","Topology":"Estrutura corporal","Weapon":"Arma","No weapon":"Sem arma","Movement":"Movimento","Stationary":"Sem deslocamento","Frames":"Quadros","Collection":"Coleção","Recent":"Recentes","Favorites":"Favoritos","No Motion Recipes match these filters.":"Nenhuma receita de movimento corresponde a estes filtros.","Effect GIF":"GIF do efeito","Effect APNG":"APNG do efeito","Sheet PNG":"Atlas PNG","Frames ZIP":"Quadros em ZIP","Metadata JSON":"Metadados JSON","Effect Pack ZIP":"Pacote do efeito em ZIP","Review / Edit Timeline":"Revisar / Editar linha do tempo","estimated":"Estimado","fallback":"Alternativa","sync":"· sincronização","Composite Stage needs at least one character animation and one Gold/Silver effect.":"A composição precisa de uma animação de personagem e de um efeito com qualidade Ouro ou Prata.","The initial candidate wave is partial and cannot resume one candidate at a time. Start a new tournament instead.":"Esta rodada já possui candidatos. Consulte os resultados existentes ou inicie uma nova comparação para gerar outros candidatos.","Needs review candidate":"Candidato precisa de revisão","Material quality gate failed":"O material não passou na verificação de qualidade","Review or download the raw candidate, retry import, or regenerate the job.":"Revise ou baixe o candidato original, tente importá-lo novamente ou gere outra versão.","The candidate was not saved as a final result. Review diagnostics and regenerate the job.":"O candidato não foi salvo como resultado final. Confira o diagnóstico antes de gerar novamente.","Bronze candidate":"Candidato Bronze","Bronze candidate already imported":"Candidato Bronze já importado","Effect imported":"Efeito importado","Effect animation results are export assets. Choose a source image or start a new generation.":"Os efeitos animados são arquivos de exportação. Escolha uma imagem de referência ou inicie outra geração.","Timeline metadata saved (PNG unchanged).":"Metadados da linha do tempo salvos. Os quadros PNG foram preservados.","Export sheet PNG, frame ZIP, GIF preview, and metadata JSON for the effect.":"Exporte o atlas PNG, os quadros em ZIP, a prévia GIF e os metadados JSON do efeito.","Quality-gated candidates cannot be downloaded as final results.":"Candidatos retidos pela verificação de qualidade não podem ser exportados como resultados finais.","Effect exports ready":"Exportações do efeito prontas","Effect output":"Resultado do efeito","Effect animation results are export assets. Select a generated or edited image before using numbered edit regions.":"Os efeitos animados são arquivos de exportação. Selecione uma imagem gerada ou editada antes de usar regiões numeradas.","This result is local procedural generation, not a Codex imagegen success result.":"Este resultado foi gerado pelo algoritmo local; não comprova uma geração de imagem pelo Codex.","Choose a preset and direction count, then generate.":"Escolha uma receita e a quantidade de direções para gerar.","Current advanced settings":"Configurações avançadas atuais","1 direction (choose direction)":"1 direção (escolha a direção)","Open advanced settings":"Abrir configurações avançadas","Animation advanced settings":"Configurações avançadas da animação","Adjust topology, frames, motion modifiers, and generation strategy only when needed.":"Ajuste o tipo de corpo, os quadros, os modificadores de movimento e a estratégia de geração quando necessário.","FPS / Frames":"Quadros por segundo / quantidade","Topology / QA":"Tipo de corpo / qualidade","Recipe status":"Estado da receita","Body topology":"Tipo de corpo","Frame budget":"Quantidade de quadros","16f / 20f are experimental. They take longer, create larger outputs, and may exceed some engine texture limits.":"16 e 20 quadros são opções experimentais: demoram mais, geram arquivos maiores e podem exceder o limite de textura de alguns motores.","Variant preview":"Prévia da variação","Frame phases":"Fases dos quadros","/ compiler":"/ compilação","sections:":"seções:","/ motion":"/ movimento","modifiers +":"modificadores +","/ dedupe":"/ duplicatas","Generation profile":"Perfil de geração","Animation generation profile":"Perfil de geração da animação","candidate":"candidato","Motion Pilot Tournament":"Comparação de pilotos de movimento","Human review gate":"Revisão visual obrigatória","Generate A/B/C in side only, choose a pilot winner, then expand the remaining":"Gere A/B/C na direção lateral, escolha o piloto vencedor e expanda as demais","directions. Fallback stays available.":"direções. A alternativa de geração permanece disponível.","Batch Matrix":"Matriz de geração em lote","Characters are rows, motions are columns. Jobs obey the global":"Os personagens formam as linhas e os movimentos, as colunas. As solicitações respeitam o limite global de","-runner limit.":"executores.","Characters":"Personagens","Motions":"Movimentos","Queue":"Enfileirar","Matrix":"Matriz","Generate with the selected preset and directions.":"Gere usando a receita e as direções selecionadas.","Actual source":"Referência da geração","The canvas shows a previous result. Generation uses only the Actual source shown above.":"A prévia mostra um resultado anterior. A geração usa somente a referência indicada acima.","Generation status & history":"Estado e histórico das gerações","Review candidates, direction state, repairs, and prior tournaments only when needed.":"Consulte os candidatos, as direções, as correções e as comparações anteriores quando necessário.","Tournament Monitor":"Acompanhamento da comparação","Persistent candidates, directions, phase, elapsed time, and warnings.":"Candidatos salvos, direções, fase, tempo decorrido e avisos.","No animation generation history yet.":"Ainda não há gerações de animação no histórico.","phase":"fase","direction outputs":"resultados direcionais","jobs":"solicitações","repairs":"correções","The initial wave is partial. Cancel this tournament and start a new one; initial candidates cannot resume individually.":"A rodada inicial está incompleta. Cancele esta comparação e inicie outra; os candidatos iniciais não podem ser retomados individualmente.","warning":"Aviso","Fallback to Balanced":"Usar geração equilibrada como alternativa","Revalidate Pilot Final":"Revalidar resultado final do piloto","Repair selected (":"Corrigir selecionados (","/2 used)":"/2 usados)","Cancel tournament":"Cancelar comparação","1. Choose Effect":"1. Escolha o efeito","Choose an effect and generate. Color, frames, and prompts stay optional.":"Escolha um efeito e gere. Cor, quadros e instruções adicionais são opcionais.","Selected effect":"Efeito selecionado","Choose Effect":"Escolher efeito","Pick a use case, then choose one variation.":"Escolha uma categoria e uma variação.","Effect categories":"Categorias de efeitos","Variation":"Variação","Use This Effect":"Usar este efeito","Effect advanced settings":"Configurações avançadas do efeito","Adjust appearance, sheet, prompts, preview, and compositing tools only when needed.":"Ajuste a aparência, o atlas, as instruções, a prévia e as ferramentas de composição quando necessário.","Appearance":"Aparência","Override only the visual style and palette.":"Ajuste o estilo visual e a paleta.","Style":"Estilo","Sheet":"Atlas","Canvas":"Área da imagem","Layout":"Disposição","Prompt (optional)":"Instruções adicionais (opcional)","The defaults are ready to use. Add only the direction you need.":"Os valores padrão estão prontos para uso. Acrescente somente as orientações necessárias.","Preview background":"Fundo da prévia","Choose a background that makes alpha edges easy to inspect.":"Escolha um fundo que facilite a inspeção das bordas transparentes.","Compositing tools (after generation)":"Ferramentas de composição (após gerar)","Use only when syncing a character and effect as separate layers.":"Use para sincronizar personagem e efeito em camadas separadas.","character":"personagem","effect":"efeito","socket + event + Pack v2":"ponto de encaixe + evento + pacote v2","Open Composite Stage":"Abrir composição","Create a character animation and a downloadable effect first.":"Crie primeiro uma animação de personagem e um efeito pronto para exportar.","2. Generate":"2. Gere o efeito","Generate a transparent effect sheet with the current selection.":"Gere um atlas de efeito transparente com a seleção atual.","Generate Effect":"Gerar efeito","Seed":"Semente","Count":"Quantidade","Size":"Tamanho","auto":"automática","low":"baixa","medium":"média","high":"alta","Providers":"Provedores","Off":"Desativado","This app does not call OpenAI APIs directly. Codex jobs are written to the local inbox":"A suíte usa o executor Codex local. As solicitações são preparadas na pasta de entrada local","GIF preview":"Prévia GIF","Building":"Preparando","Sheet preview":"Prévia do atlas","Frame timeline":"Linha do tempo","No QC warnings":"Nenhum aviso de qualidade","Split Grid":"Separar grade","Apply to Sheet":"Aplicar ao atlas","History":"Histórico","Adopted (":"Adotados (","Adopted":"Adotado","Variant Comparison":"Comparação de variações","Adopt":"Adotar","Add action preset":"Adicionar receita de ação","FPS":"Quadros/s","QC Checks":"Verificações de qualidade","Re-check":"Verificar novamente","Preview (GIF)":"Prévia GIF","Biped":"Bípede","Quadruped":"Quadrúpede","Serpentine / body contact":"Serpentino / contato corporal","Floating":"Flutuante","Winged flying":"Voo com asas","Multi-leg":"Múltiplas pernas","Idle Breathing":"Respiração em repouso","Stable 8-frame ready stance with planted feet and subtle breathing.":"Postura estável em 8 quadros, com pés apoiados e respiração suave.","Walk Cycle":"Ciclo de caminhada","Readable walking loop with contact and passing poses.":"Caminhada em ciclo com poses claras de contato e passagem.","Run Cycle":"Ciclo de corrida","Fast strides with airborne beats and strong arm drive.":"Passadas rápidas, momentos no ar e movimento forte dos braços.","Basic Attack":"Ataque básico","Grounded attack with anticipation, impact, and recovery.":"Ataque apoiado no chão, com antecipação, impacto e recuperação.","Hurt Reaction":"Reação ao dano","Short recoil, stagger, and return.":"Recuo curto, desequilíbrio e retorno.","Death / Downed":"Morte / queda","Non-gory collapse into a compact defeated pose.":"Queda sem violência gráfica até uma pose compacta de derrota.","Spell Cast":"Lançamento de magia","Raise, charge, release, and recover with compact magic.":"Elevação, carga, lançamento e recuperação com magia compacta.","Jump / Hop":"Salto","Compact in-place jump with landing and settle.":"Salto curto no lugar, com aterrissagem e estabilização.","Guard / Block":"Guarda / bloqueio","Raise guard, brace, absorb, and recover.":"Erguer a guarda, firmar o corpo, absorver o impacto e recuperar.","Victory Cheer":"Comemoração de vitória","Loopable cheer or wave with a small bounce.":"Comemoração ou aceno em ciclo, com um pequeno salto.","Interact / Pickup":"Interagir / pegar","Look, reach, inspect or pick up, then return.":"Olhar, alcançar, examinar ou pegar e depois retornar.","Ranged Attack":"Ataque à distância","Aim, release a tiny shot, and recover.":"Mirar, fazer um pequeno disparo e recuperar.","Skill Release":"Ativação de habilidade","Compact activation burst after focus.":"Ativação compacta após concentração.","Knockback":"Recuo por impacto","Large recoil, backward force, stumble, and recovery.":"Recuo amplo, impulso para trás, tropeço e recuperação.","Item Use":"Uso de item","Draw, use, read, and stow a small item.":"Pegar, usar, ler e guardar um pequeno item.","Talk / NPC Reaction":"Fala / reação de personagem","Subtle loopable talk with gestures and nods.":"Fala suave em ciclo, com gestos e movimentos de cabeça.","Dash":"Arrancada","Explosive acceleration, travel, brake, and ready recovery.":"Aceleração explosiva, deslocamento, frenagem e retorno à postura inicial.","Dodge Roll / Backstep":"Rolamento / recuo evasivo","Evade, compact transit, replant, and face the threat again.":"Esquiva, deslocamento curto, novo apoio e retorno à direção da ameaça.","Charge / Heavy Attack":"Carga / ataque pesado","Weighty charge, decisive impact, follow-through, and recovery.":"Carga pesada, impacto decisivo, continuidade do golpe e recuperação.","Combo Attack":"Sequência de ataques","Two or three linked strikes with readable rhythm and a final recovery.":"Dois ou três golpes conectados, com ritmo claro e recuperação final.","Stun":"Atordoamento","Impact loss of control, unstable hold, and partial recovery.":"Perda de controle por impacto, pausa instável e recuperação parcial.","Get Up":"Levantar","Recover support from a downed pose and return to ready.":"Recuperar o apoio após uma queda e voltar à postura inicial.","Core":"Essenciais","Locomotion":"Locomoção","Combat":"Combate","Magic":"Magia","Social":"Interação social","Utility":"Utilidade","Fast":"Rápido","Balanced":"Equilibrado","Best":"Melhor qualidade","1 candidate. Prefer time and lower usage; retry or Repair manually on failure.":"1 candidato. Prioriza rapidez e menor consumo; em caso de falha, tente novamente ou solicite o reparo.","Start 2 candidates and add candidate C only when quality is uncertain.":"Inicia 2 candidatos e adiciona o terceiro somente se a qualidade for incerta.","Start 3 candidates; accept the first strict-gate pass, otherwise use Smart Race after 2 or compare all 3.":"Inicia 3 candidatos; aceita o primeiro aprovado pelos critérios estritos ou compara os resultados após 2 ou 3 candidatos.","Intensity":"Intensidade","Tempo":"Ritmo","Weight":"Peso","Exaggeration":"Exagero","Handedness":"Mão dominante","Travel":"Deslocamento","Secondary motion":"Movimento secundário","VFX":"Efeitos","subtle":"suave","normal":"normal","strong":"forte","slow":"lento","fast":"rápido","light":"Claro","heavy":"pesado","inherit":"herdar","left":"esquerda","right":"direita","ambidextrous":"ambidestro","none":"nenhum","unarmed":"desarmado","sword":"espada","heavy-weapon":"arma pesada","polearm":"arma de haste","bow":"arco","firearm":"arma de fogo","staff":"cajado","shield":"escudo","in-place":"no lugar","short":"curto","long":"longo","Slash":"Corte","Crescent slash":"Corte crescente","Cross slash":"Corte cruzado","Spinning slash":"Corte giratório","Slash Arc":"Arco de corte","Sweeping blade or claw trails with a readable arc.":"Rastros amplos de lâmina ou garra com arco bem definido.","Hit Spark":"Faísca de impacto","Contact sparks that emphasize an impact point.":"Faíscas de contato que destacam o ponto de impacto.","Burst spark":"Explosão de faíscas","Metal spark":"Faísca metálica","Shield clash":"Impacto no escudo","Magic Cast":"Conjuração de magia","Casting circles, particles, and charge-up motion.":"Círculos de conjuração, partículas e movimento de carga.","Magic circle":"Círculo mágico","Rune glow":"Brilho de runas","Summon tell":"Sinal de invocação","Projectile":"Projétil","Traveling bullets, arrows, and energy shots.":"Balas, flechas e disparos de energia em movimento.","Energy bolt":"Disparo de energia","Fireball":"Bola de fogo","Arrow trail":"Rastro de flecha","Impact":"Impacto","Explosions, dust puffs, and ground hits.":"Explosões, nuvens de poeira e impactos no chão.","Small explosion":"Pequena explosão","Dust puff":"Nuvem de poeira","Shockwave":"Onda de choque","Telegraph / AOE":"Sinalização / área de efeito","Readable attack areas and activation timing.":"Áreas de ataque e momentos de ativação claramente indicados.","Danger ring":"Anel de perigo","Cone telegraph":"Sinalização em cone","Target zone":"Área alvo","Aura / Status":"Aura / condição","Looping buffs, debuffs, and elemental status cues.":"Ciclos de melhorias, penalidades e condições elementais.","Power aura":"Aura de poder","Poison status":"Envenenamento","Freeze status":"Congelamento","Heal / Buff":"Cura / melhoria","Rising particles, healing light, and ability boosts.":"Partículas ascendentes, luz de cura e melhorias de habilidades.","Healing rise":"Cura ascendente","Stat up":"Melhoria de atributo","Cleanse":"Purificação","Barrier / Shield":"Barreira / escudo","Defensive planes, domes, and blocking glows.":"Superfícies defensivas, cúpulas e brilhos de bloqueio.","Barrier dome":"Cúpula de barreira","Guard plane":"Superfície de defesa","Shield pulse":"Pulso de escudo","Spawn / Portal":"Aparição / portal","Spawn, despawn, and portal opening sequences.":"Sequências de aparição, desaparecimento e abertura de portal.","Portal open":"Abertura de portal","Spawn flash":"Clarão de aparição","Despawn wisps":"Partículas de desaparecimento","Movement Trail / Landing":"Rastro de movimento / aterrissagem","Dash trails, footsteps, and landing effects as separate layers.":"Rastros de arrancada, pegadas e aterrissagens em camadas separadas.","Dash trail":"Rastro de arrancada","Footstep":"Pegada","Landing dust":"Poeira de aterrissagem","Pixel Clean":"Pixel art limpa","Painterly Soft":"Pintura suave","Arcade Bold":"Arcade marcante","Anime Flare":"Clarão de anime","Cyan / White":"Ciano / branco","Violet / Gold":"Violeta / dourado","Ember Orange":"Laranja de brasa","Toxic Green":"Verde tóxico","Mono White":"Branco monocromático","4 x 2 grid":"Grade 4 × 2","4 x 4 grid":"Grade 4 × 4","Horizontal strip":"Faixa horizontal","Center":"Centro","Bottom center":"Centro inferior","Caster hand":"Mão do conjurador","Projectile center":"Centro do projétil","Impact ground":"Ponto de impacto no chão","Neutral":"Neutro","Cockpit OK.":"Suíte funcionando.","Checking Cockpit health.":"Verificando o funcionamento da suíte.","Repairing Cockpit.":"Reparando a suíte.","Cockpit repair complete. API health was rechecked and outbox recovery ran.":"Reparo da suíte concluído. A API foi verificada e a recuperação dos resultados foi executada.","Cockpit repair failed.":"Não foi possível reparar a suíte.","import":"importação","sample":"exemplo","generate":"geração","edit":"edição","ready":"pronto","off":"desligado","unavailable":"indisponível","unknown":"desconhecido","checking":"verificando","clear":"sem avisos","accepted":"aceito","rejected":"rejeitado","blocked":"bloqueado","success":"sucesso","draft":"rascunho","needs-review":"requer revisão","pilot-expanding":"expandindo piloto","quality-evaluated":"qualidade avaliada","raw-ready":"original disponível","evaluating":"avaliando","Safe Mode":"Modo seguro","Reset Mode":"Modo de restauração","Basic Character":"Personagens básicos","Profession Character":"Personagens por profissão","Monster":"Monstro","Monster Girl Chibi":"Garotas monstro chibi","Animals":"Animais","Chibi Boys":"Garotos chibi","Chibi Girls":"Garotas chibi","Beastfolk":"Povos animais","Creature":"Criatura","Prop":"Objeto","Environment":"Ambiente","Scene":"Cena","People":"Pessoas","Monsters":"Monstros","Boy Adventurer":"Garoto aventureiro","Girl Adventurer":"Garota aventureira","Young Male Hero":"Jovem herói","Young Female Hero":"Jovem heroína","Middle-Aged Male Mercenary":"Mercenário de meia-idade","Middle-Aged Female Ranger":"Patrulheira de meia-idade","Elder Male Sage":"Sábio idoso","Elder Female Herbalist":"Herbalista idosa","Androgynous Traveler":"Viajante andrógino","Small Village Child":"Criança pequena da aldeia","Large Veteran Warrior":"Guerreiro veterano corpulento","Hooded Mysterious Figure":"Figura misteriosa encapuzada","Two-Head Chibi Knight":"Cavaleiro chibi de duas cabeças de altura","Two-Head Chibi Mage":"Mago chibi de duas cabeças de altura","Two-Head Chibi Archer":"Arqueiro chibi de duas cabeças de altura","Two-Head Chibi Healer":"Curandeiro chibi de duas cabeças de altura","Two-Head Chibi Ninja":"Ninja chibi de duas cabeças de altura","Two-Head Chibi Alchemist":"Alquimista chibi de duas cabeças de altura","Two-Head Chibi Pirate":"Pirata chibi de duas cabeças de altura","Two-Head Chibi Robot":"Robô chibi de duas cabeças de altura","Two-Head Chibi Dragon Tamer":"Domador de dragões chibi de duas cabeças de altura","Cheerful starter adventurer with backpack, cloak, boots, and sword.":"Aventureiro iniciante alegre com mochila, manto, botas e espada.","Curious starter adventurer with satchel, travel cape, boots, and dagger.":"Aventureira iniciante curiosa com bolsa, capa de viagem, botas e adaga.","Light-armored hero with blue scarf and simple longsword.":"Herói de armadura leve com cachecol azul e espada longa simples.","Light-armored hero with red scarf and compact sword.":"Heroína de armadura leve com cachecol vermelho e espada compacta.","Experienced mercenary with worn leather armor, cloak, pouches, and sword.":"Mercenário experiente com armadura de couro desgastada, manto, bolsas e espada.","Composed ranger with practical leather armor, green-brown cloak, bow, and quiver.":"Patrulheira serena com armadura de couro prática, manto verde e marrom, arco e aljava.","Gentle sage with long white beard, layered robe, staff, and charm ornaments.":"Sábio gentil com barba branca longa, túnica em camadas, cajado e amuletos.","Kind herbalist with gray hair, warm shawl, herb pouch, basket, and cane.":"Herbalista bondosa com cabelo grisalho, xale quente, bolsa de ervas, cesta e bengala.","Neutral traveler with layered clothing, lowered hood, backpack, and map case.":"Viajante de aparência neutra com roupas em camadas, capuz abaixado, mochila e estojo de mapas.","Tiny non-combat NPC with simple tunic, boots, and shoulder bag.":"Personagem não jogável pequeno e sem função de combate, com túnica simples, botas e bolsa a tiracolo.","Broad veteran warrior with worn armor plates, gloves, boots, and short cape.":"Guerreiro veterano de porte largo com placas de armadura gastas, luvas, botas e capa curta.","Dark cloaked figure with hidden face, glowing charm, and quiet neutral pose.":"Figura de manto escuro, rosto oculto, amuleto luminoso e postura neutra e tranquila.","Oversized-head chibi knight with round shield and stubby sword.":"Cavaleiro chibi de cabeça grande com escudo redondo e espada curta.","Oversized-head chibi mage with starry hat, wand, and ribbon charm.":"Mago chibi de cabeça grande com chapéu estrelado, varinha e amuleto de fita.","Oversized-head chibi archer with compact bow, quiver, and feathered cap.":"Arqueiro chibi de cabeça grande com arco compacto, aljava e gorro com pena.","Oversized-head chibi healer with bell staff and medicine pouch.":"Curandeiro chibi de cabeça grande com cajado de sino e bolsa de remédios.","Oversized-head chibi ninja with scarf tails and tiny kunai.":"Ninja chibi de cabeça grande com pontas de cachecol e uma pequena kunai.","Oversized-head chibi alchemist with goggles, flask, and potion satchel.":"Alquimista chibi de cabeça grande com óculos de proteção, frasco e bolsa de poções.","Oversized-head chibi pirate with tricorn hat, coat, and tiny cutlass.":"Pirata chibi de cabeça grande com chapéu tricórnio, casaco e pequeno sabre.","Oversized-head chibi robot with antenna, tiny body, and glowing core.":"Robô chibi de cabeça grande com antena, corpo pequeno e núcleo luminoso.","Oversized-head chibi dragon tamer with horned cap and scale cape.":"Domador de dragões chibi de cabeça grande com gorro de chifres e capa de escamas.","Full-body profession character asset prepared for later animation.":"Personagem de corpo inteiro por profissão, preparado para animação posterior.","Two-head-tall chibi boy with a distinct hairstyle, outfit, and silhouette.":"Garoto chibi de duas cabeças de altura com penteado, roupa e silhueta próprios.","Two-head-tall chibi girl with a distinct hairstyle, outfit, and silhouette.":"Garota chibi de duas cabeças de altura com penteado, roupa e silhueta próprios.","Single full-body animal asset with a clean readable silhouette.":"Animal único de corpo inteiro com silhueta limpa e legível.","Single full-body beastfolk character with a clean readable silhouette.":"Personagem animal humanoide único de corpo inteiro com silhueta limpa e legível.","Single full-body monster asset with a clean readable silhouette.":"Monstro único de corpo inteiro com silhueta limpa e legível.","Cute two-head-tall monster girl source character prepared for 20x5 animation testing.":"Garota monstro chibi de duas cabeças de altura, preparada para testar animações em grade 20×5.","Clockwork Mushroom Courier":"Cogumelo mensageiro mecânico","Isolated full-body character asset with transparent output in mind.":"Personagem isolado de corpo inteiro, preparado para fundo transparente.","Forest Mage Idle Sprite":"Mago da floresta em repouso","Neutral stance character designed to be uploaded into animation generation.":"Personagem em postura neutra para usar como referência na geração de animações.","Ember Slime Companion":"Companheiro slime de brasas","Simple creature silhouette for idle, hop, and attack variants.":"Criatura de silhueta simples para variantes de repouso, salto e ataque.","Crystal Export Station":"Estação de exportação de cristais","Readable prop asset that can be separated cleanly from the canvas.":"Objeto legível que pode ser recortado da imagem com precisão.","Rainy Neon Forest Tile":"Bloco de floresta chuvosa com neon","An isolated tile example; scenery stays inside the tile footprint.":"Exemplo de bloco isolado, com o cenário contido nos limites do bloco.","Sprite-Sheet Workbench":"Bancada de atlas de sprites","Complex production-tool concept kept as one clean isolated object.":"Conceito de ferramenta de produção complexa em um único objeto isolado e legível.","Elderly Man":"Homem idoso","Elderly Woman":"Mulher idosa","Man":"Homem","Woman":"Mulher","Girl":"Garota","Boy":"Garoto","Cat":"Gato","Dog":"Cachorro","Add any hairstyle, clothes, colors, or expression you like.":"Acrescente o penteado, as roupas, as cores e a expressão que preferir.","Add any fur color, pattern, expression, or accessory you like.":"Acrescente a cor da pelagem, a padronagem, a expressão e os acessórios que preferir.","Add any fur color, breed, expression, or accessory you like.":"Acrescente a cor da pelagem, a raça, a expressão e os acessórios que preferir.","Add any shape, color, element, or personality you like.":"Acrescente a forma, a cor, o elemento e a personalidade que preferir.","Loyal Dog":"Cachorro leal","Curious Cat":"Gato curioso","Red Panda":"Panda-vermelho","Fox":"Raposa","Rabbit":"Coelho","Giant Panda":"Panda-gigante","Lion":"Leão","Tiger":"Tigre","Wolf":"Lobo","Brown Bear":"Urso-pardo","Wolf Beastfolk":"Humanoide lobo","Cat Beastfolk":"Humanoide gato","Fox Beastfolk":"Humanoide raposa","Rabbit Beastfolk":"Humanoide coelho","Tiger Beastfolk":"Humanoide tigre","Lion Beastfolk":"Humanoide leão","Bear Beastfolk":"Humanoide urso","Red Panda Beastfolk":"Humanoide panda-vermelho","Deer Beastfolk":"Humanoide cervo","Goat Beastfolk":"Humanoide cabra","Copper-Haired Scout":"Batedor de cabelo acobreado","Silver-Braided Mage":"Mago de tranças prateadas","Black-Bobbed Scholar":"Estudioso de cabelo preto curto","Sandy-Undercut Mechanic":"Mecânico de cabelo loiro com laterais raspadas","Blue-Curled Noble":"Nobre de cachos azuis","Auburn-Pigtail Farmer":"Fazendeira de marias-chiquinhas castanho-avermelhadas","Pink-Afro Dancer":"Dançarina de cabelo afro rosa","Platinum-Bobbed Winter Girl":"Garota do inverno de cabelo platinado curto","Black-Ponytail Athlete":"Atleta de rabo de cavalo preto","Violet-Braided Alchemist":"Alquimista de tranças violetas","Slime Scout Girl":"Garota slime batedora","Oni Hammer Girl":"Garota oni com martelo","Dragonkin Lancer Girl":"Garota dracônica lanceira","Harpy Wind Runner Girl":"Garota harpia corredora dos ventos","Lamia Spellblade Girl":"Garota lâmia de espada mágica","Arachne Thread Rogue Girl":"Garota aracne ladina dos fios","Centaur Guard Girl":"Garota centaura guarda","Kitsune Lantern Girl":"Garota kitsune com lanterna","Wolf Ranger Girl":"Garota loba patrulheira","Cat Thief Girl":"Garota gata ladra","Vampire Fencer Girl":"Garota vampira esgrimista","Ghost Lantern Girl":"Garota fantasma com lanterna","Fairy Archer Girl":"Garota fada arqueira","Moth Mage Girl":"Garota mariposa maga","Beetle Shield Guardian Girl":"Garota besouro guardiã de escudo","Mantis Duelist Girl":"Garota louva-a-deus duelista","Mushroom Alchemist Girl":"Garota cogumelo alquimista","Dryad Guardian Girl":"Garota dríade guardiã","Golem Defender Girl":"Garota golem defensora","Shark Pirate Girl":"Garota tubarão pirata","Classic Green Slime":"Slime verde clássico","Ember Slime":"Slime de brasas","Blue Water Slime":"Slime de água azul","Clockwork Slime":"Slime mecânico","Tiny Mushroom Monster":"Pequeno monstro cogumelo","Thorn Mushroom Monster":"Monstro cogumelo espinhoso","Goblin Scout":"Goblin batedor","Kobold Miner":"Kobold mineiro","Orc Brute":"Orc brutamontes","Lizardman Guard":"Homem-lagarto guarda","Harpy Fledgling":"Harpia filhote","Young Minotaur":"Minotauro jovem","Stone Golem":"Golem de pedra","Crystal Golem":"Golem de cristal","Candle Wisp":"Espírito de vela","Friendly Ghost":"Fantasma amigável","Skeleton Soldier":"Soldado esqueleto","Patchwork Zombie":"Zumbi remendado","Treasure Mimic":"Mímico de tesouro","Baby Dragon":"Dragão bebê","Young Wyvern":"Serpe jovem","Three-Headed Pup":"Filhote de três cabeças","Plant Sprout Monster":"Monstro broto de planta","Poison Flower Monster":"Monstro flor venenosa","Floating Eye Monster":"Monstro olho flutuante","Shadow Imp":"Diabrete das sombras","Fire Elemental":"Elemental de fogo","Water Elemental":"Elemental de água","Wind Spirit":"Espírito do vento","Earth Spirit":"Espírito da terra","Boy Warrior Apprentice":"Garoto aprendiz de guerreiro","Girl Knight Apprentice":"Garota aprendiz de cavaleira","Young Male Sword Fighter":"Jovem espadachim","Young Female Shield Guardian":"Jovem guardiã de escudo","Middle-Aged Male Axeman":"Guerreiro de machado de meia-idade","Middle-Aged Female Spear Fighter":"Lanceira de meia-idade","Elder Male Archer":"Arqueiro idoso","Elder Female Ranger":"Patrulheira idosa","Young Male Thief":"Jovem ladrão","Young Female Ninja":"Jovem ninja","Elder Male Samurai Master":"Mestre samurai idoso","Elder Female Martial Artist":"Artista marcial idosa","Boy Mage Apprentice":"Garoto aprendiz de mago","Girl Healer Apprentice":"Garota aprendiz de curandeira","Young Male Priest":"Jovem sacerdote","Young Female Witch":"Jovem bruxa","Middle-Aged Male Summoner":"Invocador de meia-idade","Middle-Aged Female Alchemist":"Alquimista de meia-idade","Elder Male Scholar":"Estudioso idoso","Elder Female Engineer":"Engenheira idosa","Young Male Blacksmith":"Jovem ferreiro","Young Female Merchant":"Jovem comerciante","Middle-Aged Male Chef":"Cozinheiro de meia-idade","Middle-Aged Female Farmer":"Fazendeira de meia-idade","Elder Male Miner":"Mineiro idoso","Elder Female Sailor":"Marinheira idosa","Young Male Bard":"Jovem bardo","Young Female Beast Tamer":"Jovem domadora de feras","Middle-Aged Male Royal Guard":"Guarda real de meia-idade","Middle-Aged Female Captain":"Capitã de meia-idade","All changes saved locally":"Todas as alterações foram salvas localmente","history":"Histórico","frames":"Quadros","animation library":"Biblioteca de animações","actions":"Ações","all local Image Cockpit state":"Estado local da suíte","Local state reset is ready.":"A redefinição do estado local está pronta.","Local state reset is ready. No large browser state was loaded.":"A redefinição do estado local está pronta. Os dados volumosos do navegador não foram carregados.","Local state safe mode: large browser storage was not loaded.":"Modo de recuperação: os dados volumosos do navegador não foram carregados.","Browser storage is above the warning threshold.":"O armazenamento do navegador ultrapassou o limite de aviso.","Could not load local browser state.":"Não foi possível carregar o estado local do navegador.","Could not load local browser state. Recovery mode is available.":"Não foi possível carregar o estado local do navegador. O modo de recuperação está disponível.","Accepted Smart Race winner recovery failed.":"Não foi possível recuperar o vencedor aceito da disputa inteligente.","Tournament resume failed.":"Não foi possível retomar o torneio.","Could not resume the initial animation candidate wave.":"Não foi possível retomar o lote inicial de candidatos da animação.","Could not read Codex runner preflight status.":"Não foi possível consultar a verificação inicial do executor Codex.","Markdown copied.":"Markdown copiado.","JSON copied.":"JSON copiado.","Copy failed.":"Não foi possível copiar.","Composite Stageにはcharacter animationとGold/Silver effectが各1件以上必要です。":"A composição precisa de pelo menos uma animação de personagem e um efeito de qualidade ouro ou prata.","Animation Review Cockpit artifact read":"Leitura de arquivos da revisão de animação","Animation Review could not load candidate artifacts.":"A revisão de animação não conseguiu carregar os arquivos dos candidatos.","Could not start the initial animation candidate wave.":"Não foi possível iniciar o lote de candidatos da animação.","Select one or more directions to Repair.":"Selecione uma ou mais direções para corrigir.","Motion Pilot final revalidation failed.":"Não foi possível concluir a revalidação do piloto de movimento.","Batch Matrix requires at least 2 sources and 2 motions.":"A matriz de produção em lote precisa de pelo menos duas referências e dois movimentos.","Effect animation results are export assets; choose a source image or start a new generation.":"Os resultados de animação de efeitos estão prontos para exportar. Selecione uma imagem de referência ou inicie outra geração.","Animation tournament started 2 initial candidates together; candidate C remains adaptive.":"O torneio de animação iniciou dois candidatos juntos; o candidato C continua adaptativo.","Duplicate Local Inbox history check complete: nothing to clean.":"Verificação de duplicatas dos resultados locais concluída: não há nada para limpar."," direction-split manifest ok.":" manifesto por direções válido.","Direction split recovery":"Recuperação da separação por direções","Runner status could not be loaded.":"Não foi possível consultar o estado do executor.","Candidate evaluation failed.":"Não foi possível avaliar o candidato."," (already imported)":" (já importado)","Initial candidates must start together.":"Os candidatos iniciais precisam começar juntos.","Not started":"Não iniciado","Review A/B/C/D":"Revisar A/B/C/D","Review A/B/C":"Revisar A/B/C","Accept Pilot Expansion":"Aceitar expansão do piloto","Accept Repair":"Aceitar correção","Negative Prompt":"Instruções do que evitar","Frame Size Consistency":"Consistência do tamanho dos quadros","All frames match":"Todos os quadros têm o mesmo tamanho","Transparent PNG":"PNG transparente","Duplicate Frame Check":"Verificação de quadros duplicados","No duplicates found":"Nenhuma duplicata encontrada","Anchor at Feet":"Âncora nos pés","fingerprinting...":"Identificando a referência...","No runner detail available.":"Nenhum detalhe do executor disponível.","Large browser local state has not been loaded.":"Os dados volumosos do navegador não foram carregados.","No summary":"Nenhum resumo","No retained local summary was found.":"Nenhum resumo local preservado foi encontrado.","normalization masked raw footline drift":"A normalização ocultou o deslocamento original dos pés","No recipe metadata was found; Idle Breathing v1 was selected explicitly.":"Não foram encontrados metadados de receita; a receita Respiração em repouso v1 foi selecionada explicitamente.","first Best candidate did not pass every requested direction":"O primeiro candidato de qualidade máxima não foi aprovado em todas as direções solicitadas","first Best candidate has quality warnings":"O primeiro candidato de qualidade máxima tem avisos de qualidade","first Best candidate score is below the strict solo threshold":"A pontuação do primeiro candidato de qualidade máxima está abaixo do mínimo para aceitação individual","first Best candidate identity is below the strict solo threshold":"A identidade do primeiro candidato de qualidade máxima está abaixo do mínimo para aceitação individual","first Best candidate has a shadow quality block":"O primeiro candidato de qualidade máxima tem um bloqueio na avaliação adicional de qualidade","first completed Best candidate passed the strict solo gate":"O primeiro candidato concluído de qualidade máxima passou nos critérios de aceitação individual","loop seam energy changes abruptly":"A intensidade muda abruptamente na emenda da repetição","alpha/opacity continuity is unstable":"A continuidade da transparência está instável","energy centroid jumps between frames":"O centro de intensidade salta entre os quadros","brightness envelope does not match the recipe":"A evolução do brilho não corresponde à receita","effect clipping or overdraw risk is high":"Há risco elevado de recorte ou sobreposição excessiva do efeito","palette changes abruptly between frames":"A paleta muda abruptamente entre os quadros","peak frame is offset from the requested event":"O quadro de pico está deslocado do evento solicitado","alpha transparency is missing from all frames":"Todos os quadros estão sem transparência","all frames are static copies":"Todos os quadros são cópias estáticas","multiple frames appear duplicated":"Há vários quadros aparentemente duplicados","Could not normalize direction split source image.":"Não foi possível normalizar a referência das direções.","Animation Review tournament is not open.":"O torneio da revisão de animações não está aberto.","Motion Pilot expansion candidate was not registered.":"O candidato de expansão do piloto não foi registrado.","Direction Repair candidate was not persisted.":"O candidato de correção da direção não foi salvo.","Accepted tournament winner is missing.":"O vencedor aceito do torneio não foi encontrado.","Accepted tournament winner candidate is missing.":"O candidato vencedor aceito do torneio não foi encontrado.","Accepted repaired artifact could not be refreshed from the published outbox final.":"Não foi possível atualizar o arquivo corrigido aceito a partir do resultado final publicado.","Local generator returned no images.":"O gerador local não retornou imagens.","Returned sprite sheet could not be split into frames.":"Não foi possível separar o atlas retornado em quadros.","Returned directional hatch-pet atlases could not be split into frames.":"Não foi possível separar os atlas direcionais do mascote em quadros.","Returned direction split animation could not be split into frames.":"Não foi possível separar a animação por direções em quadros.","Effect sheet normalization failed: canvas context unavailable.":"Não foi possível normalizar o atlas do efeito: área de desenho indisponível.","Effect frame QC failed: canvas context unavailable.":"Não foi possível verificar a qualidade dos quadros do efeito: área de desenho indisponível.","Could not compose directional hatch-pet atlas.":"Não foi possível montar o atlas direcional do mascote.","Animation pack sheet could not be split into frames.":"Não foi possível separar o atlas do pacote de animação em quadros.","No animation result is selected.":"Nenhum resultado de animação está selecionado.","Pack v2 round-trip changed the timeline metadata.":"A exportação e reimportação do pacote v2 alterou os metadados da linha do tempo.","Could not read animation sheet image.":"Não foi possível ler a imagem do atlas de animação.","Could not create transparent sprite sheet canvas.":"Não foi possível criar a área transparente do atlas.","Could not compose direction split animation sheet.":"Não foi possível montar o atlas de animação por direções.","Could not inspect animation quality reference pixels.":"Não foi possível inspecionar os pixels da referência para verificar a qualidade da animação.","Could not create animation canvas.":"Não foi possível criar a área de desenho da animação.","Could not create transparent source canvas.":"Não foi possível criar a área transparente da referência.","Dev supervisor health did not return Image Cockpit supervisor metadata.":"A verificação do supervisor não retornou os metadados esperados da suíte.","Dev supervisor repair did not complete.":"O reparo do supervisor não foi concluído.","No matching outbox result could be imported after the runner completed.":"O executor terminou, mas nenhum resultado correspondente pôde ser importado.","Direction split candidate could not be reviewed.":"Não foi possível revisar o candidato separado por direções.","Supervisor off":"Hospedagem gerenciada pelo Jarvis","Broken":"Com falha","Repairing":"Reparando","Checking":"Verificando","Runner":"Executor","Columns":"Colunas","Rows":"Linhas","Gutter":"Espaçamento","Add favorite":"Adicionar aos favoritos","Remove favorite":"Remover dos favoritos","PILOT ·":"PILOTO ·","Prompt":"Instruções","Safe modeで開く":"Abrir no modo de recuperação","通常起動へ戻る / export":"Voltar ao modo normal / exportar","localhostで新規origin":"Abrir um espaço local limpo","inbox":"caixa de entrada","example":"exemplo","annotated":"anotação","GOLD":"Ouro","SILVER":"Prata","BRONZE":"Bronze","FAILED":"Falhou","BLOCKED":"Bloqueado","gold":"Ouro","silver":"Prata","bronze":"Bronze","front":"Frente","back":"Costas","side":"Lateral","front three-quarter":"Frente em 3/4","back three-quarter":"Costas em 3/4","startup":"Preparação","active":"Ativo","impact":"Impacto","recovery":"Recuperação","loop-point":"Ponto de repetição","recipe":"Receita","user":"Usuário","frame":"Quadro","queued":"Na fila","running":"Executando","completed":"Concluído","failed":"Falhou","cancelled":"Cancelado","waiting":"Aguardando","review":"Revisão","pilot-review":"Revisão do piloto","pivots":"Pivôs","anchors":"Âncoras","sockets":"Pontos de encaixe","hitboxes":"Áreas de impacto","hurtboxes":"Áreas vulneráveis","winner":"Vencedor","hold":"Aguardar","reject":"Rejeitar","piloting":"Gerando piloto","repairing":"Corrigindo","checkerboard":"Transparência","dark":"Escuro","game":"Jogo","start":"Início","peak":"Pico","core":"Essenciais","locomotion":"Locomoção","combat":"Combate","magic":"Magia","social":"Interação","utility":"Utilitários","pass":"Aprovado","fail":"Falha","identity-drift":"Alteração da identidade","palette-drift":"Alteração da paleta","silhouette-break":"Quebra da silhueta","footline-drift":"Deslocamento dos pés","loop-seam":"Emenda da repetição","phase-mismatch":"Fases incompatíveis","motion-weak":"Movimento pouco perceptível","over-correction":"Correção excessiva","clipping":"Recorte indevido","static-copy":"Cópia estática","motion":"movimento","walk motion":"movimento de caminhada","idle breathing motion":"respiração em repouso","subtle motion":"movimento sutil","footline":"pés","hover-height":"altura de flutuação","wing-beat":"batida das asas","ground-contact":"contato com o chão","body-contact":"contato do corpo","order":"ordem","durations":"durações","events":"eventos","order, durations":"ordem, durações","order, events":"ordem, eventos","durations, events":"durações, eventos","order, durations, events":"ordem, durações, eventos"};
const jarvisVisiblePatterns = [["^Bronze candidate quarantined for ([\\s\\S]*?): ([\\s\\S]*?)\\. Raw direction files are available for diagnostics, but no final history or download item was added\\.$","Candidato bronze de ${0} separado para revisão: ${1}. Os arquivos originais das direções estão disponíveis para diagnóstico; nenhum resultado final foi adicionado ao histórico ou disponibilizado para baixar."],["^([\\s\\S]*?): tournament winner ([\\s\\S]*?) \\(([\\s\\S]*?)\\), score ([\\s\\S]*?), warnings ([\\s\\S]*?)\\. Compared ([\\s\\S]*?) usable candidate([\\s\\S]*?) and cancelled ([\\s\\S]*?) remaining runner([\\s\\S]*?)\\. direction-split manifest ok\\.$","${0}: vencedor ${1} (${2}), nota ${3}, avisos ${4}. Foram comparados ${5} candidatos utilizáveis e cancelados ${7} executores restantes. Manifesto por direção verificado."],["^Animation quality gate failed: ([\\s\\S]*?) is a single-direction intermediate sheet \\(([\\s\\S]*?)x([\\s\\S]*?)\\), not a final direction-split animation result\\.$","A verificação de qualidade falhou: ${0} é um atlas intermediário de uma direção (${1}×${2}); o resultado final com todas as direções ainda não está pronto."],["^Animation quality gate failed for ([\\s\\S]*?): ([\\s\\S]*?)\\. Candidate files are quarantined; no history or final download item was added\\.$","A animação de ${0} não passou na verificação de qualidade: ${1}. Os candidatos foram separados para revisão; nenhum resultado final foi adicionado ao histórico ou disponibilizado para baixar."],["^Effect quality gate failed for ([\\s\\S]*?): ([\\s\\S]*?) Candidate files are quarantined; no history or final download item was added\\.$","O efeito de ${0} não passou na verificação de qualidade: ${1} Os candidatos foram separados para revisão; nenhum resultado final foi adicionado ao histórico ou disponibilizado para baixar."],["^Experimental Motion Pilot started ([\\s\\S]*?) ([\\s\\S]*?)-only candidates together\\. Human review is required before expansion\\.$","Piloto experimental iniciado com ${0} candidatos apenas para ${1}. A expansão exige revisão visual."],["^Motion Pilot A/B/C is ready for human review: ([\\s\\S]*?)\\. Choose a winner to expand or use Balanced fallback\\.$","Pilotos A/B/C prontos para revisão visual: ${0}. Escolha um vencedor para expandir ou use a alternativa equilibrada."],["^([\\s\\S]*?) localStorage cleared\\. IndexedDB was unavailable or blocked; close other tabs and retry if needed\\.$","${0}: armazenamento local limpo. O IndexedDB estava indisponível ou bloqueado; feche outras abas e tente novamente, se necessário."],["^Animation tournament started A/B/C together \\(([\\s\\S]*?)\\) with First Qualified Wins and Smart Race fallback\\.$","Torneio A/B/C iniciado simultaneamente (${0}); prioriza o primeiro aprovado e compara os demais quando necessário."],["^sourceId remap after local-inbox history dedupe, then latest ([\\s\\S]*?) frames plus selected/action frames$","Referências atualizadas após agrupar importações locais idênticas; últimos ${0} quadros, além dos selecionados ou associados às ações"],["^idle breathing motion too static across directions \\(([\\s\\S]*?)/([\\s\\S]*?) readable rows, ([\\s\\S]*?) average frame change\\)$","A respiração em repouso está muito estática nas direções (${0}/${1} linhas legíveis, ${2} de mudança média entre quadros)"],["^Direction Repair ready for review: ([\\s\\S]*?)\\. Untargeted directions remain locked until acceptance\\.$","Correção de direção pronta para revisão: ${0}. As outras direções permanecem preservadas até a aceitação."],["^Batch Matrix queued: ([\\s\\S]*?) sources × ([\\s\\S]*?) motions, ([\\s\\S]*?) initial candidates\\. Concurrency is capped at ([\\s\\S]*?)\\.$","Lote na fila: ${0} referências × ${1} movimentos, ${2} candidatos iniciais. Limite de ${3} execuções simultâneas."],["^Runner slots are full \\(([\\s\\S]*?)/([\\s\\S]*?)\\)\\. The Pilot expansion will stay in review until a slot is free\\.$","Os executores estão ocupados (${0}/${1}). A expansão do piloto continuará em revisão até haver uma vaga."],["^Runner slots are full \\(([\\s\\S]*?)/([\\s\\S]*?)\\)\\. Pilot expansion will remain in review until a slot is free\\.$","Executores ocupados (${0}/${1}). A expansão do piloto aguardará revisão até haver uma vaga."],["^deduped exact local-inbox imports, then latest ([\\s\\S]*?) results plus adopted/selected results$","Importações locais idênticas agrupadas; últimos ${0} resultados, além dos adotados ou selecionados"],["^Animation quality gate failed for ([\\s\\S]*?): ([\\s\\S]*?)\\. No history or final download item was added\\.$","A animação de ${0} não passou na verificação de qualidade: ${1}. Nenhum resultado final foi adicionado ao histórico ou disponibilizado para baixar."],["^Browser storage warning: ([\\s\\S]*?) used\\. Old local results will be retained conservatively\\.$","Aviso de armazenamento: ${0} utilizados. Os resultados locais antigos serão preservados de forma conservadora."],["^Direction split import failed for ([\\s\\S]*?): missing ([\\s\\S]*?)\\. Outbox files are still available\\.$","Falha ao importar as direções de ${0}: falta ${1}. Os arquivos produzidos continuam disponíveis."],["^Effect QC quarantined for ([\\s\\S]*?): ([\\s\\S]*?)\\. No history or final download item was added\\.$","O efeito de ${0} foi separado por falha na verificação de qualidade: ${1}. Nenhum resultado final foi adicionado ao histórico ou disponibilizado para baixar."],["^Winner is accepted, but loser runner cancellation is still unconfirmed: ([\\s\\S]*?)$","O vencedor foi aceito, mas o cancelamento dos outros executores ainda não foi confirmado: ${0}"],["^Unknown recipe version '([\\s\\S]*?)' for '([\\s\\S]*?)'; using current v([\\s\\S]*?) with explicit warning\\.$","Versão '${0}' desconhecida para a receita '${1}'; usando a versão atual v${2} com este aviso."],["^Motion Pilot frozen for this trial: ([\\s\\S]*?)\\. Balanced fallback ([\\s\\S]*?) registered\\.$","Piloto pausado nesta tentativa: ${0}. Alternativa equilibrada ${1} registrada."],["^Direction Repair accepted: ([\\s\\S]*?)\\. ([\\s\\S]*?) untargeted direction hashes unchanged\\.$","Correção de direção aceita: ${0}. Os hashes de ${1} direções não selecionadas foram preservados."],["^([\\s\\S]*?) Run diagnostics or repair Cockpit before starting API-backed work\\.$","${0} Execute o diagnóstico ou repare a suíte antes de iniciar uma geração."],["^Bronze candidate for ([\\s\\S]*?): ([\\s\\S]*?)\\. Raw direction files are still available\\.$","Candidato bronze para ${0}: ${1}. Os arquivos originais das direções continuam disponíveis."],["^Motion Pilot candidates failed\\. Balanced fallback is available: ([\\s\\S]*?)\\.$","Os candidatos do piloto falharam. A alternativa equilibrada está disponível: ${0}."],["^Duplicate Local Inbox history cleaned: ([\\s\\S]*?) duplicate result([\\s\\S]*?) removed\\.$","Histórico local limpo: ${0} resultados duplicados removidos."],["^Recover Results complete: no new formal outbox images were found\\.([\\s\\S]*?)$","Recuperação concluída: nenhuma nova imagem final foi encontrada.${0}"],["^([\\s\\S]*?) cleared\\. Open the normal URL to start with a clean local state\\.$","${0}: limpeza concluída. Reabra a suíte para começar com o estado local redefinido."],["^([\\s\\S]*?): Character transparency damage detected \\(([\\s\\S]*?)% interior alpha hole\\)$","${0}: transparência do personagem danificada (${1}% de vazios internos)"],["^idle breathing motion is subtle overall \\(([\\s\\S]*?) average frame change\\)$","A respiração em repouso está sutil no conjunto (${0} de mudança média entre quadros)"],["^Recovered local history: removed ([\\s\\S]*?) duplicate Local Inbox result([\\s\\S]*?)\\.$","Histórico recuperado: ${0} resultados locais duplicados removidos."],["^Recovered ([\\s\\S]*?) outbox result([\\s\\S]*?)\\. ([\\s\\S]*?) already imported or not recoverable\\.([\\s\\S]*?)([\\s\\S]*?)$","Recuperados ${0} resultados. ${2} já importados ou sem recuperação disponível.${3}${4}"],["^([\\s\\S]*?): Chroma key removal failed \\(([\\s\\S]*?)% residue in character component\\)$","${0}: falha ao remover a cor de recorte (${1}% de resíduos no personagem)"],["^Motion Pilot final revalidated across ([\\s\\S]*?) published directions: ([\\s\\S]*?)$","Piloto final revalidado nas ${0} direções publicadas: ${1}"],["^Unknown recipe id '([\\s\\S]*?)'; resolved by action or safe default\\.$","Receita '${0}' desconhecida; selecionada pela ação ou pelo padrão de recuperação."],["^Legacy preset '([\\s\\S]*?)' had no recipe version; migrated to v([\\s\\S]*?)\\.$","A predefinição legada '${0}' não tinha versão da receita; migrada para v${1}."],["^Animation tournament started ([\\s\\S]*?)/([\\s\\S]*?) candidates together \\(([\\s\\S]*?)\\)\\.$","Torneio iniciado com ${0}/${1} candidatos simultâneos (${2})."],["^([\\s\\S]*?) frame([\\s\\S]*?) may contain baked checkerboard or matte pixels$","${0} quadros podem conter quadriculado de transparência incorporado ou pixels de fundo"],["^Recovered accepted Smart Race winner after reload: ([\\s\\S]*?)\\.$","Candidato vencedor recuperado após recarregar: ${0}."],["^Direction split manifest has unsupported schema: ([\\s\\S]*?)$","O manifesto das direções usa um formato não suportado: ${0}"],["^latest ([\\s\\S]*?) results plus adopted/selected results$","Últimos ${0} resultados, além dos adotados ou selecionados"],["^([\\s\\S]*?): no character pixels remained after cleanup$","${0}: nenhum pixel do personagem permaneceu após a limpeza"],["^Accepted Smart Race winner recovery failed: ([\\s\\S]*?)$","Falha ao recuperar o candidato vencedor: ${0}"],["^Animation Review loaded: ([\\s\\S]*?)/([\\s\\S]*?) candidates ready\\.$","Revisão carregada: ${0}/${1} candidatos prontos."],["^latest ([\\s\\S]*?) frames plus selected/action frames$","Últimos ${0} quadros, além dos selecionados ou associados às ações"],["^Motion Pilot winner adopted: ([\\s\\S]*?)\\. Expanding ([\\s\\S]*?)\\.$","Piloto vencedor adotado: ${0}. Expandindo ${1}."],["^Direction Repair failed Quality Gate v2: ([\\s\\S]*?)$","A correção da direção não passou na verificação de qualidade v2: ${0}"],["^Started all ([\\s\\S]*?) initial candidates together\\.$","Os ${0} candidatos iniciais foram iniciados juntos."],["^([\\s\\S]*?): could not create direction cell canvas$","${0}: não foi possível criar a área da célula de direção"],["^Balanced tournament added candidate C: ([\\s\\S]*?)\\.$","O torneio equilibrado adicionou o candidato C: ${0}."],["^Ignored temporary or QA outbox result: ([\\s\\S]*?)$","Resultado temporário ou de verificação ignorado: ${0}"],["^([\\s\\S]*?): no primary character component found$","${0}: componente principal do personagem não encontrado"],["^normalization translation peaked at ([\\s\\S]*?)px$","O deslocamento aplicado pela normalização chegou a ${0} px"],["^([\\s\\S]*?): normalized source image from ([\\s\\S]*?)x([\\s\\S]*?) to ([\\s\\S]*?)x([\\s\\S]*?)$","${0}: referência normalizada de ${1}×${2} para ${3}×${4}"],["^waiting for stable verified artifacts([\\s\\S]*?)$","aguardando arquivos estáveis e verificados${0}"],["^([\\s\\S]*?) uses explicit direction override: ([\\s\\S]*?)\\.$","${0} usa configurações específicas da direção: ${1}."],["^([\\s\\S]*?): ([\\s\\S]*?) too small \\(([\\s\\S]*?) average frame change\\)$","${0}: ${1} insuficiente (${2} de mudança média entre quadros)"],["^Manual tournament winner adopted: ([\\s\\S]*?)$","Vencedor selecionado adotado: ${0}"],["^([\\s\\S]*?): ([\\s\\S]*?) \\(waiting for direction split: ([\\s\\S]*?)\\)$","${0}: ${1} (aguardando separação por direção: ${2})"],["^one or more frames do not match ([\\s\\S]*?)x([\\s\\S]*?)$","Um ou mais quadros não correspondem a ${0}×${1}"],["^([\\s\\S]*?): detached component ([\\s\\S]*?)px from body$","${0}: componente separado a ${1} px do corpo"],["^([\\s\\S]*?): ([\\s\\S]*?) absent \\(([\\s\\S]*?) average frame change\\)$","${0}: ${1} ausente (${2} de mudança média entre quadros)"],["^normalization resized ([\\s\\S]*?)% of frames$","A normalização redimensionou ${0}% dos quadros"],["^([\\s\\S]*?): possible interior alpha hole ([\\s\\S]*?)%$","${0}: possível vazio interno de transparência (${1}%)"],["^Animation tournament cancelled: ([\\s\\S]*?)$","Torneio de animação cancelado: ${0}"],["^([\\s\\S]*?): ([\\s\\S]*?) \\(waiting for final manifest\\)$","${0}: ${1} (aguardando manifesto final)"],["^([\\s\\S]*?): low ([\\s\\S]*?) \\(([\\s\\S]*?) average frame change\\)$","${0}: ${1} pouco perceptível (${2} de mudança média entre quadros)"],["^Direction Repair started: ([\\s\\S]*?) \\(([\\s\\S]*?)\\)\\.$","Correção de direção iniciada: ${0} (${1})."],["^sheet size ([\\s\\S]*?)x([\\s\\S]*?) does not match ([\\s\\S]*?)x([\\s\\S]*?)$","O atlas de ${0}×${1} não corresponde a ${2}×${3}"],["^Animation tournament failed: ([\\s\\S]*?)$","Falha no torneio de animação: ${0}"],["^([\\s\\S]*?): ([\\s\\S]*?) \\(waiting for effect sheet\\)$","${0}: ${1} (aguardando atlas do efeito)"],["^Start all ([\\s\\S]*?) initial candidates$","Iniciar os ${0} candidatos iniciais"],["^sheet normalized from ([\\s\\S]*?)x([\\s\\S]*?) to ([\\s\\S]*?)x([\\s\\S]*?)$","Atlas normalizado de ${0}×${1} para ${2}×${3}"],["^frame count ([\\s\\S]*?) does not match ([\\s\\S]*?)$","A quantidade de quadros (${0}) não corresponde a ${1}"],["^Animation Pack v2 exported: ([\\s\\S]*?)$","Pacote de animação v2 exportado: ${0}"],["^Direction split QA failed: ([\\s\\S]*?)$","A verificação das direções falhou: ${0}"],["^Recipe '([\\s\\S]*?)' v0 migrated to v([\\s\\S]*?)\\.$","Receita '${0}' v0 migrada para v${1}."],["^([\\s\\S]*?): could not inspect pixels$","${0}: não foi possível inspecionar os pixels"],["^Tournament resume failed: ([\\s\\S]*?)$","Falha ao retomar o torneio: ${0}"],["^([\\s\\S]*?): missing direction image$","${0}: imagem da direção ausente"],["^([\\s\\S]*?): bbox height variation ([\\s\\S]*?)%$","${0}: variação de ${1}% na altura do personagem"],["^Direction Repair failed: ([\\s\\S]*?)$","Falha ao corrigir a direção: ${0}"],["^VFX Composite export: ([\\s\\S]*?) / ([\\s\\S]*?)$","Exportação da composição de efeitos: ${0} / ${1}"],["^([\\s\\S]*?) frame([\\s\\S]*?) touch cell bounds$","${0} quadros encostam nos limites da célula"],["^([\\s\\S]*?): bbox width variation ([\\s\\S]*?)%$","${0}: variação de ${1}% na largura do personagem"],["^([\\s\\S]*?): feet touch cell bottom$","${0}: os pés encostam no limite inferior da célula"],["^([\\s\\S]*?): expected ([\\s\\S]*?) cells, got ([\\s\\S]*?)$","${0}: esperadas ${1} células, recebidas ${2}"],["^([\\s\\S]*?): blank normalized cell$","${0}: célula normalizada vazia"],["^raw height variation ([\\s\\S]*?)%$","Variação da altura original: ${0}%"],["^raw footline drift ([\\s\\S]*?)px$","Deslocamento original dos pés: ${0} px"],["^raw width variation ([\\s\\S]*?)%$","Variação da largura original: ${0}%"],["^([\\s\\S]*?): ([\\s\\S]*?) \\(already imported\\)$","${0}: ${1} (já importado)"],["^Human review saved: ([\\s\\S]*?)$","Revisão visual salva: ${0}"],["^([\\s\\S]*?): expected ([\\s\\S]*?)x([\\s\\S]*?), got ([\\s\\S]*?)x([\\s\\S]*?)$","${0}: esperado ${1}×${2}, recebido ${3}×${4}"],["^([\\s\\S]*?) retained, largest ([\\s\\S]*?)$","${0} preservados; maior item: ${1}"],["^raw center drift ([\\s\\S]*?)px$","Deslocamento do centro original: ${0} px"],["^([\\s\\S]*?) retained, largest ([\\s\\S]*?)$","${0} preservados, maior item: ${1}"],["^([\\s\\S]*?): chroma residue ([\\s\\S]*?)%$","${0}: ${1}% de resíduos da cor de recorte"],["^([\\s\\S]*?): center drift ([\\s\\S]*?)px$","${0}: deslocamento do centro de ${1} px"],["^([\\s\\S]*?) sample animation$","Animação de exemplo: ${0}"],["^ · ([\\s\\S]*?)f Experimental$"," · ${0}q Experimental"],["^([\\s\\S]*?) Direction Repair$","Correção da direção ${0}"],["^([\\s\\S]*?): top margin ([\\s\\S]*?)px$","${0}: margem superior de ${1} px"],["^Timeline frame ([\\s\\S]*?)$","Quadro da linha do tempo ${0}"],["^Frame ([\\s\\S]*?) duration$","Duração do quadro ${0}"],["^Selected frame ([\\s\\S]*?)$","Quadro selecionado ${0}"],["^([\\s\\S]*?)f Experimental$","${0}q Experimental"],["^ / ([\\s\\S]*?) unimported$"," / ${0} não importados"],["^Candidate ([\\s\\S]*?): ([\\s\\S]*?)$","Candidato ${0}: ${1}"],["^([\\s\\S]*?) event frame$","Quadro do evento ${0}"],["^([\\s\\S]*?): ([\\s\\S]*?) drift ([\\s\\S]*?)px$","${0}: deslocamento de ${1} de ${2} px"],["^([\\s\\S]*?) \\(repair: ([\\s\\S]*?)\\)$","${0} (reparo: ${1})"],["^([\\s\\S]*?) mismatches$","${0} divergências"],["^([\\s\\S]*?) duplicates$","${0} duplicatas"],["^Candidate ([\\s\\S]*?)$","Candidato ${0}"],["^, ([\\s\\S]*?) warning([\\s\\S]*?)$",", ${0} avisos"],["^Selected: ([\\s\\S]*?)$","Selecionado: ${0}"],["^([\\s\\S]*?) warnings$","${0} avisos"],["^([\\s\\S]*?) selected$","${0} selecionado"],["^([\\s\\S]*?)/([\\s\\S]*?) frames$","${0}/${1} quadros"],["^([\\s\\S]*?) example$","Exemplo: ${0}"],["^([\\s\\S]*?) ([\\s\\S]*?) frame ([\\s\\S]*?)$","${0} ${1} quadro ${2}"],["^([\\s\\S]*?) warning$","${0} avisos"],["^([\\s\\S]*?) frames([\\s\\S]*?)$","${0} quadros${1}"],["^([\\s\\S]*?)/([\\s\\S]*?) items$","${0}/${1} itens"],["^([\\s\\S]*?) frame ([\\s\\S]*?)$","${0} quadro ${1}"]].map(([source, target]) => ({pattern:new RegExp(source),target}));
function jarvisUiText<T>(value: T): T {
  if (typeof value !== "string") return value;
  const exact = jarvisVisibleLabels[value];
  if (exact) return exact as T;
  if (value.startsWith("Effect: ")) {
    const category = value.slice(8);
    return ("Efeito: " + (jarvisVisibleLabels[category] ?? category)) as T;
  }
  const effectMeta = /^(GOLD|SILVER|BRONZE) \/ (\d+)f \/ (\d+)px$/.exec(value);
  if (effectMeta) return (jarvisVisibleLabels[effectMeta[1]] + " / " + effectMeta[2] + " quadros / " + effectMeta[3] + " px") as T;
  const effectImport = /^(?:Effect imported|Efeito importado): (.+) \((gold|silver|bronze|failed|blocked)(?:, (\d+) warnings?)?\)$/.exec(value);
  if (effectImport) return ("Efeito importado: " + effectImport[1] + " (" + jarvisVisibleLabels[effectImport[2]]
    + (effectImport[3] ? ", " + effectImport[3] + " avisos" : "") + ")") as T;
  for (const {pattern,target} of jarvisVisiblePatterns) {
    const match = pattern.exec(value);
    if (match) return target.replace(/\$\{(\d+)\}/g, (_all, index: string) => {
      const captured = match[Number(index)+1] ?? "";
      return captured.split("; ").map((part) => jarvisUiText(part)).join("; ");
    }) as T;
  }
  const prefixes: Record<string,string> = {"Timeline frame ":"Quadro da linha do tempo ","Selected frame ":"Quadro selecionado ","Frame ":"Quadro ","VFX Composite export:":"Exportação da composição de efeitos:"};
  for (const [prefix, translated] of Object.entries(prefixes)) if (value.startsWith(prefix)) return (translated + value.slice(prefix.length)) as T;
  return value
    .replace(/^(startup|active|impact|recovery|loop-point) \((recipe|user)\)$/, (_match, name: string, origin: string) => jarvisVisibleLabels[name] + " (" + jarvisVisibleLabels[origin] + ")")
    .replace(/^Cockpit OK\. (\d+) outbox results? available for Recover Results\.$/, "Suíte funcionando. $1 resultados disponíveis para recuperar.")
    .replace(/^(\d+) warning$/, "$1 avisos")
    .replace(/^ \/ (\d+) unimported$/, " / $1 não importados") as T;
}
import { bindController, reportOperationError } from "../jarvis-worker";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleX,
  Download,
  Eye,
  EyeOff,
  Pause,
  Play,
  Save,
  StepBack,
  StepForward,
  Wrench,
  X
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { AnimationQualityReportV2 } from "./types";
import {
  animationReviewAutomaticReason,
  animationReviewDimensionScores,
  animationReviewQcMatrix,
  animationReviewReasonTags,
  buildAnimationCalibrationExport,
  emptyAnimationHumanReview,
  upsertAnimationHumanReviewDecision
} from "./lib/animationReview";
import type {
  AnimationHumanReview,
  AnimationReviewDecision,
  AnimationReviewHeatMode,
  AnimationReviewQcCell
} from "./lib/animationReview";
import { downloadBlob } from "./lib/image";

export interface AnimationReviewCandidateView {
  jobId: string;
  label: string;
  state: string;
  score?: number;
  warningCount?: number;
  reason?: string;
  error?: string;
  sheets: Record<string, string | undefined>;
  quality?: AnimationQualityReportV2;
  repairDirections?: string[];
}

export interface AnimationReviewManifestView {
  tournamentId: string;
  sourceFingerprint: string;
  generationProfile: string;
  requestedDirections: string[];
  winnerCandidateId?: string;
  thirdCandidateReason?: string;
  state: string;
}

interface AnimationReviewCockpitProps {
  language: "ja" | "en";
  manifest: AnimationReviewManifestView;
  candidates: AnimationReviewCandidateView[];
  frameCount: number;
  sourceDataUrl?: string;
  initialReview?: AnimationHumanReview;
  initialDirection?: string;
  loading?: boolean;
  adoptDisabled?: boolean;
  adoptDisabledReason?: string;
  onClose: () => void;
  onSaveReview: (review: AnimationHumanReview) => Promise<void>;
  onAdopt: (jobId: string) => Promise<void>;
  onRepairDirection: (direction: string) => Promise<void>;
}

const heatModes: Array<{ id: AnimationReviewHeatMode; ja: string; en: string }> = [
  { id: "foot-drift", ja: "接地ずれ", en: "Foot drift" },
  { id: "bbox", ja: "BBox変動", en: "BBox" },
  { id: "identity", ja: "同一性", en: "Identity" },
  { id: "loop-seam", ja: "ループ継ぎ目", en: "Loop seam" },
  { id: "static-copy", ja: "静止コピー", en: "Static copy" },
  { id: "clipping", ja: "クリップ", en: "Clipping" }
];

const speedOptions = [0.5, 1, 1.5, 2];

function statusIcon(status: "pass" | "warning" | "fail") {
  if (status === "pass") return <CheckCircle2 size={14} aria-hidden="true" />;
  if (status === "warning") return <AlertTriangle size={14} aria-hidden="true" />;
  return <CircleX size={14} aria-hidden="true" />;
}

function frameSheetStyle(frameIndex: number, frameCount: number, correction?: { scale: number; translateX: number; translateY: number }): CSSProperties {
  const safeCount = Math.max(1, frameCount);
  const columns = safeCount <= 4 ? safeCount : safeCount === 6 ? 3 : 4;
  const rows = Math.ceil(safeCount / columns);
  const column = frameIndex % columns;
  const row = Math.floor(frameIndex / columns);
  return {
    width: `${columns * 100}%`,
    height: `${rows * 100}%`,
    maxWidth: "none",
    maxHeight: "none",
    transform: `translate(${-column * (100 / columns)}%, ${-row * (100 / rows)}%)${correction ? ` translate(${correction.translateX}px, ${correction.translateY}px) scale(${correction.scale})` : ""}`,
    transformOrigin: `${((column + 0.5) / columns) * 100}% ${((row + 0.5) / rows) * 100}%`
  };
}

function CandidateFrame({
  candidate,
  direction,
  frameIndex,
  frameCount,
  normalized,
  sourceDataUrl,
  sourceOpacity,
  showOnion,
  showDiff,
  showGround,
  showPivot,
  showSocket,
  showHitbox,
  showHurtbox,
  background
}: {
  candidate: AnimationReviewCandidateView;
  direction: string;
  frameIndex: number;
  frameCount: number;
  normalized: boolean;
  sourceDataUrl?: string;
  sourceOpacity: number;
  showOnion?: boolean;
  showDiff?: boolean;
  showGround?: boolean;
  showPivot?: boolean;
  showSocket?: boolean;
  showHitbox?: boolean;
  showHurtbox?: boolean;
  background?: "checker" | "light" | "dark";
}) {
  const sheet = candidate.sheets[direction];
  const correction = candidate.quality?.normalizationCorrection.frames.find((entry) => entry.direction === direction && entry.frameIndex === frameIndex);
  const previousIndex = (frameIndex - 1 + Math.max(1, frameCount)) % Math.max(1, frameCount);
  return (
    <div className={`review-frame-stage background-${background ?? "checker"}`}>
      {sheet ? (
        <>
          {showOnion ? (
            <div className="review-frame-layer review-frame-onion" aria-hidden="true">
              <img src={sheet} alt="" style={frameSheetStyle(previousIndex, frameCount)} />
            </div>
          ) : null}
          {showDiff ? (
            <div className="review-frame-layer review-frame-diff" aria-hidden="true">
              <img src={sheet} alt="" style={frameSheetStyle(previousIndex, frameCount)} />
            </div>
          ) : null}
          <div className="review-frame-layer">
            <img
              src={sheet}
              alt={jarvisUiText(`${candidate.label} ${direction} frame ${frameIndex + 1}`)}
              style={frameSheetStyle(frameIndex, frameCount, normalized ? correction : undefined)}
            />
          </div>
        </>
      ) : (
        <div className="review-frame-missing"><CircleX size={20} aria-hidden="true" /><span> Nenhum arquivo </span></div>
      )}
      {sourceDataUrl && sourceOpacity > 0 ? <img className="review-source-overlay" src={sourceDataUrl} alt="" style={{ opacity: sourceOpacity }} aria-hidden="true" /> : null}
      {showGround ? <span className="review-ground-line" aria-label="Linha do chão" /> : null}
      {showPivot ? <span className="review-pivot-marker" aria-label="Marcador do pivô">P</span> : null}
      {showSocket ? <span className="review-socket-marker" aria-label="Marcador do ponto de encaixe">S</span> : null}
      {showHitbox ? <span className="review-hitbox" aria-label="Sobreposição da área de impacto"> Impacto </span> : null}
      {showHurtbox ? <span className="review-hurtbox" aria-label="Sobreposição da área vulnerável"> Vulnerável </span> : null}
    </div>
  );
}

export function AnimationReviewCockpit({
  language,
  manifest,
  candidates,
  frameCount,
  sourceDataUrl,
  initialReview,
  initialDirection,
  loading,
  adoptDisabled = false,
  adoptDisabledReason,
  onClose,
  onSaveReview,
  onAdopt,
  onRepairDirection
}: AnimationReviewCockpitProps) {
  const ja = language === "ja";
  const dialogRef = useRef<HTMLElement>(null);
  const firstFocusRef = useRef<HTMLButtonElement>(null);
  const artifactDirections = manifest.requestedDirections.filter((requestedDirection) =>
    candidates.some((candidate) => Boolean(candidate.sheets[requestedDirection]))
  );
  const directions = artifactDirections.length > 0
    ? artifactDirections
    : manifest.requestedDirections.length > 0 ? manifest.requestedDirections : ["front"];
  const [directionIndex, setDirectionIndex] = useState(Math.max(0, directions.indexOf(initialDirection ?? "")));
  const [frameIndex, setFrameIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [normalized, setNormalized] = useState(true);
  const [sourceOpacity, setSourceOpacity] = useState(0);
  const [activeCandidateId, setActiveCandidateId] = useState(candidates[0]?.jobId ?? "");
  const [heatMode, setHeatMode] = useState<AnimationReviewHeatMode>("foot-drift");
  const [selectedCell, setSelectedCell] = useState<AnimationReviewQcCell>();
  const [review, setReview] = useState<AnimationHumanReview>(initialReview ?? emptyAnimationHumanReview(new Date().toISOString()));
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [actionState, setActionState] = useState<"idle" | "adopting" | "repairing">("idle");
  const [actionError, setActionError] = useState("");
  const [showOnion, setShowOnion] = useState(false);
  const [showDiff, setShowDiff] = useState(false);
  const [showGround, setShowGround] = useState(true);
  const [showPivot, setShowPivot] = useState(false);
  const [showSocket, setShowSocket] = useState(false);
  const [showHitbox, setShowHitbox] = useState(false);
  const [showHurtbox, setShowHurtbox] = useState(false);
  const [background, setBackground] = useState<"checker" | "light" | "dark">("checker");
  const [reducedMotion, setReducedMotion] = useState(false);
  const direction = directions[Math.min(directionIndex, directions.length - 1)];
  const activeCandidate = candidates.find((candidate) => candidate.jobId === activeCandidateId) ?? candidates[0];
  const candidateCompareTitle = candidates.length > 3 ? "Candidate A/B/C/D · Sync Compare" : "Candidate A/B/C · Sync Compare";
  const activeDecision = review.decisions.find((entry) => entry.jobId === activeCandidate?.jobId);
  const qcCells = useMemo(
    () => animationReviewQcMatrix(activeCandidate?.quality, directions, frameCount, heatMode),
    [activeCandidate?.quality, directions, frameCount, heatMode]
  );

  useEffect(() => {
    setDirectionIndex(Math.max(0, directions.indexOf(initialDirection ?? "")));
    setFrameIndex(0);
  }, [manifest.tournamentId, initialDirection]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => {
      setReducedMotion(media.matches);
      if (media.matches) setPlaying(false);
    };
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  useEffect(() => {
    if (!playing || reducedMotion) return;
    const timer = window.setInterval(() => setFrameIndex((current) => (current + 1) % Math.max(1, frameCount)), Math.max(45, 125 / speed));
    return () => window.clearInterval(timer);
  }, [frameCount, playing, reducedMotion, speed]);

  useEffect(() => {
    firstFocusRef.current?.focus();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        setFrameIndex((current) => (current - 1 + Math.max(1, frameCount)) % Math.max(1, frameCount));
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        setFrameIndex((current) => (current + 1) % Math.max(1, frameCount));
      } else if (event.key === " " && !/INPUT|TEXTAREA|SELECT|BUTTON/.test((event.target as HTMLElement).tagName)) {
        event.preventDefault();
        setPlaying((current) => !current);
      } else if (event.key === "Tab") {
        const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])') ?? []);
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [frameCount]);

  function updateDecision(decision: AnimationReviewDecision) {
    if (!activeCandidate) return;
    setReview((current) => upsertAnimationHumanReviewDecision(current, {
      jobId: activeCandidate.jobId,
      decision,
      reasonTags: activeDecision?.reasonTags ?? [],
      note: activeDecision?.note ?? ""
    }));
    setSaveState("idle");
  }

  function updateDecisionDetail(patch: { reasonTags?: string[]; note?: string }) {
    if (!activeCandidate) return;
    setReview((current) => upsertAnimationHumanReviewDecision(current, {
      jobId: activeCandidate.jobId,
      decision: activeDecision?.decision ?? "hold",
      reasonTags: patch.reasonTags ?? activeDecision?.reasonTags ?? [],
      note: patch.note ?? activeDecision?.note ?? ""
    }));
    setSaveState("idle");
  }

  async function saveReview() {
    setSaveState("saving");
    try {
      await onSaveReview(review);
      setSaveState("saved");
    } catch {
      setSaveState("error");
    }
  }

  async function adoptWinner() {
    const winnerId = review.manualWinnerJobId ?? activeCandidate?.jobId;
    if (!winnerId) return;
    setActionState("adopting");
    setActionError("");
    try {
      await onSaveReview(review.manualWinnerJobId ? review : upsertAnimationHumanReviewDecision(review, { jobId: winnerId, decision: "winner", reasonTags: [], note: "" }));
      await onAdopt(winnerId);
      onClose();
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Candidate adoption failed."); reportOperationError(error);
    } finally {
      setActionState("idle");
    }
  }

  function exportCalibration() {
    const data = buildAnimationCalibrationExport({
      tournamentId: manifest.tournamentId,
      sourceFingerprint: manifest.sourceFingerprint,
      winnerJobId: manifest.winnerCandidateId,
      review,
      reports: Object.fromEntries(candidates.map((candidate) => [candidate.jobId, candidate.quality]))
    });
    downloadBlob(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }), `${manifest.tournamentId}-calibration.json`);
  }

  async function repairDirection() {
    setActionState("repairing");
    setActionError("");
    try {
      await onRepairDirection(selectedCell?.direction ?? direction);
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Direction Repair could not start."); reportOperationError(error);
    } finally {
      setActionState("idle");
    }
  }

  const correction = selectedCell?.correction;
  useEffect(() => bindController({ state: {directionIndex,frameIndex,playing,speed,normalized,sourceOpacity,activeCandidateId,heatMode,selectedCell,review,saveState,actionState,actionError,showOnion,showDiff,showGround,showPivot,showSocket,showHitbox,showHurtbox,background,reducedMotion,directions,candidates,frameCount,qcCells}, setters: {"directionIndex": setDirectionIndex,"frameIndex": setFrameIndex,"playing": setPlaying,"speed": setSpeed,"normalized": setNormalized,"sourceOpacity": setSourceOpacity,"activeCandidateId": setActiveCandidateId,"heatMode": setHeatMode,"selectedCell": setSelectedCell,"review": setReview,"saveState": setSaveState,"actionState": setActionState,"actionError": setActionError,"showOnion": setShowOnion,"showDiff": setShowDiff,"showGround": setShowGround,"showPivot": setShowPivot,"showSocket": setShowSocket,"showHitbox": setShowHitbox,"showHurtbox": setShowHurtbox,"background": setBackground,"reducedMotion": setReducedMotion}, methods: {updateDecision,updateDecisionDetail,saveReview,adoptWinner,exportCalibration,repairDirection,close: onClose} }, "review"));

  return (
    <JarvisDialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <JarvisDialogContent showCloseButton={false} data-jarvis-suite-dialog=""
        ref={dialogRef}
        className="animation-review-cockpit"
        role="dialog"
        aria-modal="true"
        aria-labelledby="animation-review-title"
        aria-describedby="animation-review-summary"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="animation-review-header">
          <div>
            <span className="eyebrow"> Revisão de animações </span>
            <h2 id="animation-review-title">{jarvisUiText(ja ? "候補比較・QC・採用レビュー" : "Candidate compare, QC and adoption")}</h2>
            <p id="animation-review-summary">{jarvisUiText(manifest.generationProfile)} · {jarvisUiText(directions.length)}  direções · {jarvisUiText(frameCount)}  quadros · {jarvisUiText(manifest.state)}</p>
          </div>
          <div className="animation-review-header-actions">
            <JarvisButton variant="outline" ref={firstFocusRef} type="button" className="secondary-button" onClick={exportCalibration}><Download size={15} aria-hidden="true" /> JSON de calibração </JarvisButton>
            <JarvisButton variant="outline" type="button" className="icon-button" aria-label={jarvisUiText(ja ? "レビューを閉じる" : "Close review")} onClick={onClose}><X size={18} aria-hidden="true" /></JarvisButton>
          </div>
        </header>

        <div className="animation-review-toolbar" aria-label={jarvisUiText(ja ? "同期再生コントロール" : "Synchronized playback controls")}>
          <JarvisButton variant="outline" type="button" className="icon-button" aria-label={jarvisUiText(playing ? "Pause" : "Play")} aria-pressed={playing} disabled={reducedMotion} onClick={() => setPlaying((current) => !current)}>{playing ? <Pause size={16} /> : <Play size={16} />}</JarvisButton>
          <JarvisButton variant="outline" type="button" className="icon-button" aria-label="Quadro anterior" onClick={() => setFrameIndex((current) => (current - 1 + frameCount) % frameCount)}><StepBack size={16} /></JarvisButton>
          <strong>F {jarvisUiText(frameIndex + 1)}/{jarvisUiText(frameCount)}</strong>
          <JarvisButton variant="outline" type="button" className="icon-button" aria-label="Próximo quadro" onClick={() => setFrameIndex((current) => (current + 1) % frameCount)}><StepForward size={16} /></JarvisButton>
          <label><span> Velocidade </span><JarvisNativeSelect value={speed} onChange={(event) => setSpeed(Number(event.target.value))}>{speedOptions.map((value) => <JarvisNativeSelectOption key={value} value={value}>{jarvisUiText(value)}x</JarvisNativeSelectOption>)}</JarvisNativeSelect></label>
          <div className="segmented-control compact" aria-label="Direção">
            <JarvisButton variant="outline" type="button" className="icon-button" aria-label="Direção anterior" onClick={() => setDirectionIndex((current) => (current - 1 + directions.length) % directions.length)}><ChevronLeft size={15} /></JarvisButton>
            <strong>{jarvisUiText(direction)}</strong>
            <JarvisButton variant="outline" type="button" className="icon-button" aria-label="Próxima direção" onClick={() => setDirectionIndex((current) => (current + 1) % directions.length)}><ChevronRight size={15} /></JarvisButton>
          </div>
          <JarvisButton variant="outline" type="button" className={normalized ? "active" : ""} aria-pressed={normalized} onClick={() => setNormalized((current) => !current)}>{jarvisUiText(normalized ? "Normalized" : "Raw")}</JarvisButton>
          <label className="source-opacity-control"><span>{jarvisUiText(ja ? "原画重ね" : "Source")} {jarvisUiText(Math.round(sourceOpacity * 100))}%</span><JarvisInput type="range" min="0" max="1" step="0.1" value={sourceOpacity} disabled={!sourceDataUrl} onChange={(event) => setSourceOpacity(Number(event.target.value))} /></label>
          {reducedMotion ? <small className="reduced-motion-note"><Pause size={13} />{jarvisUiText(ja ? "視差軽減設定により自動再生停止" : "Autoplay paused by reduced motion")}</small> : null}
        </div>

        <main className="animation-review-scroll">
          <section className="review-section" aria-labelledby="candidate-compare-title">
            <div className="review-section-heading">
              <div><span className="step-number">1</span><h3 id="candidate-compare-title">{jarvisUiText(candidateCompareTitle)}</h3></div>
              <p>{jarvisUiText(ja ? "全候補を同じ方向・フレーム・速度で比較するニャ" : "Compare every candidate at the same direction, frame and speed.")}</p>
            </div>
            {loading ? <div className="review-loading" role="status"> Carregando arquivos dos candidatos… </div> : null}
            <div className={`candidate-compare-grid candidate-count-${Math.min(candidates.length, 3)}`}>
              {candidates.map((candidate) => {
                const selected = candidate.jobId === activeCandidate?.jobId;
                const decision = review.decisions.find((entry) => entry.jobId === candidate.jobId);
                const scores = animationReviewDimensionScores(candidate.quality);
                return (
                  <article key={candidate.jobId} className={`review-candidate-card${selected ? " selected" : ""}${candidate.error ? " failed" : ""}`}>
                    <JarvisButton variant="outline" type="button" className="candidate-card-select" aria-pressed={selected} onClick={() => setActiveCandidateId(candidate.jobId)}>
                      <span><strong>{jarvisUiText(candidate.label)}</strong><small>{jarvisUiText(candidate.state)} · {jarvisUiText(candidate.jobId.slice(-8))}</small></span>
                      <span className={`status-badge status-${candidate.error ? "fail" : candidate.warningCount ? "aviso" : "pass"}`}>{candidate.error ? <CircleX size={13} /> : candidate.warningCount ? <AlertTriangle size={13} /> : <CheckCircle2 size={13} />}{jarvisUiText(candidate.error ? "Failed" : `${candidate.warningCount ?? 0} warnings`)}</span>
                    </JarvisButton>
                    <CandidateFrame candidate={candidate} direction={direction} frameIndex={frameIndex} frameCount={frameCount} normalized={normalized} sourceDataUrl={sourceDataUrl} sourceOpacity={sourceOpacity} background={background} />
                    <div className="candidate-dimension-grid">
                      {scores.map((score) => <span key={score.id} className={`dimension-score status-${score.status}`}>{jarvisUiText(statusIcon(score.status))}<b>{jarvisUiText(score.label)}</b><em>{jarvisUiText(score.score ?? "—")}</em></span>)}
                    </div>
                    <p className="candidate-auto-reason">{jarvisUiText(candidate.error ?? candidate.reason ?? animationReviewAutomaticReason(candidate.quality))}</p>
                    {candidate.repairDirections?.length ? <p className="candidate-repair-note"><Wrench size={13} /> Correção: {jarvisUiText(candidate.repairDirections.join(", "))}</p> : null}
                    {decision ? <span className={`manual-decision decision-${decision.decision}`}>{decision.decision === "winner" ? <Check size={13} /> : decision.decision === "reject" ? <CircleX size={13} /> : <Eye size={13} />}{jarvisUiText(decision.decision)}</span> : null}
                  </article>
                );
              })}
            </div>
            {manifest.thirdCandidateReason ? <p className="review-callout"><AlertTriangle size={15} /> Candidato C: {jarvisUiText(manifest.thirdCandidateReason)}</p> : null}
          </section>

          <section className="review-section review-workbench" aria-labelledby="review-workbench-title">
            <div className="review-section-heading">
              <div><span className="step-number">2</span><h3 id="review-workbench-title"> Revisão visual </h3></div>
              <p>{jarvisUiText(ja ? "自動スコアを上書きせず、人の判断を別レイヤーで保存するニャ" : "Store human judgment as a separate calibration layer.")}</p>
            </div>
            {activeCandidate ? (
              <div className="human-review-grid">
                <fieldset>
                  <legend>{jarvisUiText(activeCandidate.label)}</legend>
                  <div className="segmented-control decision-control">
                    {(["winner", "hold", "reject"] as AnimationReviewDecision[]).map((decision) => <JarvisButton variant="outline" key={decision} type="button" className={activeDecision?.decision === decision ? "active" : ""} aria-pressed={activeDecision?.decision === decision} onClick={() => updateDecision(decision)}>{jarvisUiText(decision)}</JarvisButton>)}
                  </div>
                  <div className="reason-tag-grid" aria-label="Motivos da rejeição">
                    {animationReviewReasonTags.map((tag) => {
                      const checked = activeDecision?.reasonTags.includes(tag) ?? false;
                      return <label key={tag} className={checked ? "checked" : ""}><JarvisCheckbox  checked={checked} onCheckedChange={() => updateDecisionDetail({ reasonTags: checked ? activeDecision?.reasonTags.filter((item) => item !== tag) : [...(activeDecision?.reasonTags ?? []), tag] })} />{jarvisUiText(tag)}</label>;
                    })}
                  </div>
                </fieldset>
                <label className="review-note"><span>{jarvisUiText(ja ? "レビュー・メモ" : "Review note")}</span><JarvisTextarea value={activeDecision?.note ?? ""} maxLength={1000} placeholder={jarvisUiText(ja ? "目視判断、採用理由、再生成条件" : "Visual judgment, adoption reason, regeneration condition")} onChange={(event) => updateDecisionDetail({ note: event.target.value })} /><small>{jarvisUiText((activeDecision?.note ?? "").length)}/1000</small></label>
                <div className="review-save-panel" aria-live="polite">
                  <JarvisButton variant="outline" type="button" className="secondary-button" disabled={saveState === "saving"} onClick={() => void saveReview()}><Save size={15} />{jarvisUiText(saveState === "saving" ? "Saving…" : ja ? "判断を保存" : "Save review")}</JarvisButton>
                  <span className={`save-state save-${saveState}`}>{jarvisUiText(saveState === "saved" ? (ja ? "保存済み" : "Saved") : saveState === "error" ? (ja ? "保存失敗" : "Save failed") : "")}</span>
                </div>
              </div>
            ) : null}
          </section>

          <section className="review-section" aria-labelledby="qc-matrix-title">
            <div className="review-section-heading">
              <div><span className="step-number">3</span><h3 id="qc-matrix-title"> Qualidade por direção e quadro </h3></div>
              <div className="qc-legend"><span className="status-pass"><CheckCircle2 size={13} /> Aprovado </span><span className="status-warning"><AlertTriangle size={13} /> Aviso </span><span className="status-fail"><CircleX size={13} /> Falha </span></div>
            </div>
            <div className="qc-mode-tabs" role="tablist" aria-label="Mapa de qualidade">
              {heatModes.map((mode) => <JarvisButton variant="outline" key={mode.id} type="button" role="tab" aria-selected={heatMode === mode.id} className={heatMode === mode.id ? "active" : ""} onClick={() => setHeatMode(mode.id)}>{jarvisUiText(ja ? mode.ja : mode.en)}</JarvisButton>)}
            </div>
            <div className="qc-matrix-scroll">
              <table className="qc-matrix">
                <thead><tr><th> Direção </th>{Array.from({ length: frameCount }, (_, index) => <th key={index}>F{jarvisUiText(index + 1)}</th>)}</tr></thead>
                <tbody>{directions.map((itemDirection) => <tr key={itemDirection}><th>{jarvisUiText(itemDirection)}</th>{qcCells.filter((cell) => cell.direction === itemDirection).map((cell) => <td key={cell.frameIndex}><JarvisButton variant="outline" type="button" className={`qc-cell status-${cell.status}${selectedCell?.direction === cell.direction && selectedCell.frameIndex === cell.frameIndex ? " selected" : ""}`} aria-label={jarvisUiText(cell.label)} title={jarvisUiText(cell.label)} onClick={() => { setSelectedCell(cell); setDirectionIndex(directions.indexOf(cell.direction)); setFrameIndex(cell.frameIndex); }}>{jarvisUiText(statusIcon(cell.status))}<span>{jarvisUiText(cell.value)}</span></JarvisButton></td>)}</tr>)}</tbody>
              </table>
            </div>
            {selectedCell && activeCandidate ? (
              <div className="qc-cell-inspector">
                <div><strong>{jarvisUiText(selectedCell.direction)} · F{jarvisUiText(selectedCell.frameIndex + 1)}</strong><span className={`status-badge status-${selectedCell.status}`}>{jarvisUiText(statusIcon(selectedCell.status))}{jarvisUiText(selectedCell.status)} · {jarvisUiText(selectedCell.value)}</span></div>
                <div className="qc-frame-comparison">
                  <figure><CandidateFrame candidate={activeCandidate} direction={selectedCell.direction} frameIndex={selectedCell.frameIndex} frameCount={frameCount} normalized={false} sourceOpacity={0} background={background} /><figcaption> Original </figcaption></figure>
                  <figure><CandidateFrame candidate={activeCandidate} direction={selectedCell.direction} frameIndex={selectedCell.frameIndex} frameCount={frameCount} normalized sourceOpacity={0} background={background} /><figcaption> Normalizado </figcaption></figure>
                  <figure><CandidateFrame candidate={activeCandidate} direction={selectedCell.direction} frameIndex={selectedCell.frameIndex} frameCount={frameCount} normalized sourceOpacity={0} showDiff background="dark" /><figcaption> Diferença entre quadros vizinhos </figcaption></figure>
                </div>
                <dl className="correction-facts"><div><dt> Escala </dt><dd>{jarvisUiText(correction?.scale.toFixed(3) ?? "—")}</dd></div><div><dt> Deslocamento X </dt><dd>{jarvisUiText(correction?.translateX.toFixed(1) ?? "—")}</dd></div><div><dt> Deslocamento Y </dt><dd>{jarvisUiText(correction?.translateY.toFixed(1) ?? "—")}</dd></div></dl>
                <div className="repair-actions"><JarvisButton variant="outline" type="button" className="secondary-button" disabled={actionState !== "idle"} onClick={() => void repairDirection()}><Wrench size={15} />{jarvisUiText(actionState === "repairing" ? "Starting…" : `${selectedCell.direction} Direction Repair`)}</JarvisButton><JarvisButton variant="outline" type="button" disabled title={jarvisUiText(ja ? "部分フレーム修復は生成器がフレーム範囲固定を保証できないため未対応" : "Frame-range repair is disabled because the generator cannot yet preserve the untouched frame range.")}> Corrigir intervalo de quadros </JarvisButton><small>{jarvisUiText(ja ? "フレーム範囲修復は未変更フレーム保証がないため無効" : "Disabled until untouched-frame preservation can be verified.")}</small></div>
              </div>
            ) : null}
          </section>

          <section className="review-section" aria-labelledby="preview-studio-title">
            <div className="review-section-heading"><div><span className="step-number">4</span><h3 id="preview-studio-title"> Inspeção visual </h3></div><p>{jarvisUiText(ja ? "ゲーム座標とループ継ぎ目を同時確認するニャ" : "Inspect game coordinates and the loop seam together.")}</p></div>
            <div className="preview-studio-grid">
              <div className="preview-studio-stage">
                {activeCandidate ? <CandidateFrame candidate={activeCandidate} direction={direction} frameIndex={frameIndex} frameCount={frameCount} normalized={normalized} sourceDataUrl={sourceDataUrl} sourceOpacity={sourceOpacity} showOnion={showOnion} showDiff={showDiff} showGround={showGround} showPivot={showPivot} showSocket={showSocket} showHitbox={showHitbox} showHurtbox={showHurtbox} background={background} /> : null}
                <div className="loop-seam-strip"><span> Transição da repetição </span><b>F{jarvisUiText(frameCount)}</b><ChevronRight size={14} /><b>F1</b><em>{jarvisUiText(activeCandidate?.quality?.loopSeamScore ?? "—")}</em></div>
              </div>
              <div className="preview-overlay-controls">
                {[{ label: "Onion", value: showOnion, set: setShowOnion }, { label: "Prev/next diff", value: showDiff, set: setShowDiff }, { label: "Ground", value: showGround, set: setShowGround }, { label: "Pivot", value: showPivot, set: setShowPivot }, { label: "Socket", value: showSocket, set: setShowSocket }, { label: "Hitbox", value: showHitbox, set: setShowHitbox }, { label: "Hurtbox", value: showHurtbox, set: setShowHurtbox }].map((control) => <JarvisButton variant="outline" key={control.label} type="button" className={control.value ? "active" : ""} aria-pressed={control.value} onClick={() => control.set(!control.value)}>{control.value ? <Eye size={14} /> : <EyeOff size={14} />}{jarvisUiText(control.label)}</JarvisButton>)}
                <label><span> Fundo </span><JarvisNativeSelect value={background} onChange={(event) => setBackground(event.target.value as typeof background)}><JarvisNativeSelectOption value="checker"> Quadriculado </JarvisNativeSelectOption><JarvisNativeSelectOption value="light"> Claro </JarvisNativeSelectOption><JarvisNativeSelectOption value="dark"> Escuro </JarvisNativeSelectOption></JarvisNativeSelect></label>
              </div>
            </div>
          </section>
        </main>

        <footer className="animation-review-footer">
          <div aria-live="polite"><strong>{jarvisUiText(review.manualWinnerJobId ? `${candidates.find((candidate) => candidate.jobId === review.manualWinnerJobId)?.label ?? "Candidate"} selected` : ja ? "採用候補を選択してください" : "Select an adoption candidate")}</strong><small className={actionError ? "review-action-error" : ""}>{jarvisUiText(actionError || adoptDisabledReason || (ja ? "自動採用理由と警告を確認してから確定" : "Confirm automatic reasons and warnings before adoption."))}</small></div>
          <JarvisButton variant="outline" type="button" className="secondary-button" onClick={onClose}>{jarvisUiText(ja ? "閉じる" : "Close")}</JarvisButton>
          <JarvisButton variant="outline" type="button" className="primary-button" disabled={!review.manualWinnerJobId || actionState !== "idle" || adoptDisabled} title={jarvisUiText(adoptDisabled ? adoptDisabledReason : undefined)} onClick={() => void adoptWinner()}><Check size={16} />{jarvisUiText(actionState === "adopting" ? "Adopting…" : ja ? "選択候補を採用" : "Adopt selected candidate")}</JarvisButton>
        </footer>
      </JarvisDialogContent>
    </JarvisDialog>
  );
}
