# Roadmap

Formato: milestones versionados. Cada release é um escopo fechado;
quando todos os checks estão marcados, lança-se a versão e abre-se a
próxima.

## Princípio: paridade CLI ↔ TUI

Toda feature exposta ao usuário ganha **duas interfaces na mesma
milestone**: o subcomando CLI (`cdx <verbo>`) e a tela equivalente
dentro da TUI (`cdx tui`). Os dois caminhos consomem o mesmo módulo
de domínio — a divergência fica restrita à camada de apresentação.
Por isso os itens abaixo, quando listam apenas o verbo CLI, implicam
também a tela TUI correspondente.

Exceção (rara, sempre justificada):

- Leitor de livros (v0.9) — só faz sentido na TUI.

## TUI: navegação global

A tela de abertura da TUI (mesma da welcome reusada do módulo
compartilhado) é o ponto de entrada e lista as **seções top-level**
como links navegáveis (↑/↓ percorrem a lista, Enter entra).
Cada seção corresponde a um conjunto coerente de verbos CLI:

1. **Library** — listar/visualizar/remover livros (`cdx ls`,
   `cdx show`, `cdx rm`) [v0.1]
2. **Search** — busca full-text + filtros (`cdx search`) [v0.3]
3. **Catalogs** — registry de catálogos (`cdx catalog ls`/`use`/
   `rm` + wizard de `init`/`add`) [v0.1]
4. **Devices** — sync com ereaders (`cdx device ls`, `device books`,
   `device alias`, `push`, `pull`, `sync`) [v0.4]

Seções de milestones futuros aparecem na lista com sufixo
"(v0.X)" e ficam desabilitadas (Enter sobre elas não navega) até
serem entregues no milestone correspondente.

**Atalho global — command palette via `:`**: de qualquer tela, `:`
abre um input no rodapé (estilo vim). Comandos disponíveis:
`:library`, `:catalogs`, `:search`, `:devices`, mais `:quit`
(alias de `q`, convenção vim — não é rebind do exit, é uma forma
alternativa). Tab completa pelo prefixo único mais curto (`:l`,
`:c`, `:s`, `:d`, `:q`). Enter executa; Esc cancela e volta o
foco pra tela ativa.

Restrições:

- As teclas reservadas (`q`, `Ctrl+C` pra sair; `Esc`, `Enter` pra
  navegação in-screen) seguem valendo. O palette só captura
  texto enquanto está aberto — fora dele, `q` continua sendo o
  exit imediato.
- O palette **não substitui** o help contextual `?`, que continua
  per-screen documentando atalhos da tela ativa.

## v0.1 — MVP catálogo

Catálogo local independente, sem device sync, sem Calibre. O usuário
escolhe onde cada catálogo vive (path qualquer — pode ser um repo
git); o cdx mantém um registro multi-catálogo em
`$XDG_CONFIG_HOME/cdx/config.toml` com o catálogo "atual".

- [x] Definir esquema inicial do catálogo (SQLite — tabela `books`)
- [x] `cdx catalog init <name> <path>` — cria DB + `books/` no path e
      registra o catálogo
- [x] `cdx catalog add <name> <path>` — registra um catálogo já
      existente no path informado
- [x] `cdx catalog ls` — lista catálogos registrados (marca atual e
      `(missing)` quando o path sumiu do disco)
- [x] `cdx catalog use <name>` — troca o catálogo atual
- [x] `cdx catalog rm <name>` — remove do registro (flag `--purge`
      pra apagar os arquivos)
- [x] TUI: tela "Catalogs" — lista catálogos (atual marcado,
      `(missing)` quando o path sumiu), permite `use` (Enter) e `rm`
      (com confirmação + opção de purgar). A welcome é sempre a home;
      a tela Catalogs é acessada via menu ou `:catalogs`.
- [x] TUI: wizard "New catalog" — fluxo único que cobre `init` (cria
      DB + `books/`) e `add` (registra path existente), com nome,
      path e descrição opcional
- [x] TUI: estender welcome com menu das 4 seções top-level
      (Library e Catalogs ativas; Search "(v0.3)" e Devices "(v0.4)"
      desabilitadas até seus milestones)
- [x] TUI: command palette `:` — overlay no rodapé com input +
      tab-complete; registra `:library` (stub se a tela Library
      ainda não estiver pronta), `:catalogs`, `:quit`/`:q`; demais
      seções registram-se em seus milestones
- [x] `cdx add <file>...` — importa EPUB/PDF/MOBI/AZW3, extrai metadados
      básicos e renomeia o arquivo armazenado como `Author_-_Title.ext`
      sanitizado; formatos fora da lista são recusados com mensagem clara
- [x] `cdx ls` — lista livros (id, título, autor, formato)
- [x] `cdx inspect <id|título>` — exibe metadados detalhados; aceita id
      numérico ou título exato (case-insensitive); título ambíguo retorna
      erro listando os ids candidatos
    - [ ] autocomplete dinâmico de nome/id no shell — adiado pra v1.0
          (`clap_complete` feature `unstable-dynamic`)
- [x] `cdx rm <id|título>` — remove do catálogo e apaga o arquivo; flag
      `--keep` move o arquivo pra cwd em vez de apagar (sufixa `.1`, `.2`
      em colisão)
- [x] Logging configurável via `RUST_LOG` (`tracing-subscriber` lê
      `RUST_LOG`; `-v/-vv/-vvv` ajusta o default sem precisar exportar)
- [x] Tela de boas vindas em módulo compartilhado, exibida quando `cdx`
      roda sem subcomando (mesmo conteúdo será reusado pela TUI)
- [x] `cdx tui` — esqueleto ratatui + tela de boas vindas reusando o
      módulo compartilhado (prova o ciclo CLI↔TUI; demais telas entram
      junto com seus respectivos comandos nos milestones seguintes)

## v0.2 — Edição de metadados

Ciclo de embed: qualquer edit (`cdx edit` ou TUI `e`) marca o livro como
`embed_status = 'pending'`; o sync (`cdx embed sync`, TUI `w` ou
`Ctrl+W`) embeda no arquivo e marca `synced` (EPUB/PDF) ou
`unsupported` (MOBI/AZW3, não-retentável).

- [x] `cdx edit <id>` — abre `$EDITOR` com TOML dos metadados; valida
      no parse e reaproveita `handle_update` (que reseta `embed_status`
      para `pending`); tempfile preservado em caso de erro
- [x] `cdx tag <id> <tag>...` / `cdx untag <id> <tag>... [--all]` — campo
      "Tags" no modal de edit da TUI (multi, comma-separated); coluna "tags"
      em `cdx ls` humano e JSON; embed_status volta a `pending` só quando o
      conjunto muda; `--all` em `untag` zera todas as tags
- [x] `cdx rate <id> <0-5>` — TUI: campo "Rating" no modal (validado 0–5);
      CLI aceita 0–5 e trata `0` como "limpar"
- [x] `cdx series <id> <name> [--index N]` — TUI: campos "Series" +
      "Index" no modal; CLI tem `--clear` pra remover (sem `<name>`)
- [x] `cdx embed sync` — embeda metadados em todos os livros `pending`;
      imprime progresso linha-a-linha + summary final
- [x] TUI: embed de metadados em arquivo (EPUB/PDF) via tecla `w` no
      Inspect — MOBI/AZW3 retorna status "embed not supported"
- [x] Migration `0002_metadata.sql` — colunas `description`,
      `series_name`, `series_index`, `rating`, `isbn`, `publisher`,
      `language`, `published_date` em `books`; tabela `tags` + `book_tags`
- [x] Migration `0004_embed_state.sql` — colunas `embed_status` +
      `embed_synced_at` em `books`
- [x] Migration `0005_content_hash.sql` + dedup no `cdx add` — fingerprint
      SHA-256 por livro (tabela `book_hashes`); EPUB ganha hash de conteúdo
      estável (ignora o OPF reescrito pelo embed), demais formatos hash do
      arquivo + hash pós-embed acumulado; duplicata é pulada com aviso, `--force`
      reimporta; backfill best-effort dos livros existentes
- [x] Extração no `cdx add` estendida (EPUB/MOBI/PDF) para popular os
      novos campos quando disponíveis no arquivo

## v0.3 — Busca e filtros

- [x] `cdx search <query>` — substring case-insensitive em título/autor/tags
      (whitespace = AND tokens; reusa o renderer do `ls` pra humano e JSONL)
- [x] Flags `--author`, `--tag`, `--series`, `--rating`
- [x] Saída `--json` pra compor com `jq`/scripts
- [x] TUI: registrar `:search` no command palette + ativar link
      "Search" na welcome — vira o "modo filtrado" da tela Library:
      `/` filtra por texto (tokens AND, como no CLI) e `:search` abre o
      wizard com campos de texto/autor/tag/série/rating; Esc limpa o filtro

## v0.4 — Kindle sync (USB)

Sync com ereaders montados via USB, com suporte a múltiplos devices
simultâneos. Cada device é identificado pelo serial estável (lido do
descritor USB via sysfs) e pode ganhar um **apelido** — é o apelido
que os comandos usam no dia a dia.

A identidade de um livro entre catálogo e device tem duas camadas:

1. **Exata** — a tabela de sync state (`device_books`) registra cada
   livro que o cdx enviou/trouxe (book_id ↔ path no device + checksum
   SHA-256 + tamanho/mtime). Pra esses, não há adivinhação.
2. **Por metadado** — pra arquivos que chegaram no device por fora,
   o match é **título + autor normalizados** (casefold, NFKD sem
   diacríticos, pontuação e whitespace colapsados), nunca o arquivo:
   o formato pode variar entre as pontas (EPUB local vs AZW3 no
   device) e hash não sobrevive a conversão. Variações pequenas
   ("Café" vs "Cafe") casam; sem fuzzy matching — ambiguidade real
   (dois candidatos pro mesmo match) nunca se resolve sozinha, vira
   conflito pra decisão manual.

- [x] Detectar Kindles montados via USB mass storage (Linux), com
      suporte a múltiplos devices simultâneos; a identidade estável de
      cada device é o **serial** lido do descritor USB em sysfs
      (`idVendor` 1949 = Amazon/Lab126 é o gate; `documents/` +
      `system/` no mount são só sanity check). Em outros SOs a
      detecção compila e retorna lista vazia
- [x] Migration `0007_devices.sql` — tabela `devices` (`serial` PK,
      `alias`, `last_seen_at`) + tabela `device_books` (sync state:
      `device_serial`, `book_id`, `device_path`, `hash`, `size`,
      `mtime`, `synced_at`); devices vivem no catalog.db, então o
      apelido é por catálogo
- [x] `cdx device ls` — lista os devices detectados e os conhecidos
      (alias, serial, mount path quando conectado, espaço livre,
      contagem de livros); humano + JSONL
- [x] `cdx device alias <serial|alias> <new-alias>` — define/renomeia
      o apelido; na primeira detecção de um device sem apelido o
      serial é usado como fallback nas listagens
- [x] `cdx device books [--device <alias>]` — lista livros do device
      lendo metadados dos arquivos (não só filename), com a coluna de
      presença ("both" / "device only") via sync state + match
      normalizado; humano + JSONL
- [x] Seleção de device: flag `--device <alias>` em `device books`,
      `push`, `pull` e `sync`. Um device conectado → default
      implícito; dois ou mais sem o flag → erro claro listando os
      candidatos (nunca escolher sozinho)
- [x] `cdx push <id|título> [--device <alias>]` — copia arquivo do
      catálogo pro device e grava o sync state (hash/size/mtime); sem
      `<id|título>` abre um seletor interativo (setas/`j``k` + Enter)
      listando os livros do catálogo
- [x] `cdx pull <path> [--device <alias>]` — importa livro do device
      reusando o pipeline do `cdx add` (incluindo dedup por hash) e
      grava o sync state; sem `<path>` abre um seletor interativo
      (setas/`j``k` + Enter) listando os livros do device
- [x] Verificação de sync: o diff confere cada entrada do sync state
      pelo fast-path tamanho + mtime; divergência marca o livro como
      `modified` (re-push é oferecido no plano). `--verify` força
      SHA-256 completo (USB é lento — full hash só sob demanda).
      Entrada cujo arquivo sumiu do device vira `missing`
- [x] `cdx sync [--device <alias>]` — diff bidirecional **iterativo**:
      computa o plano (faltantes em cada ponta, `modified`, `missing`,
      conflitos de match) e confirma item a item, estilo `git add -p`
      (`y` aplica / `n` pula / `a` aceita o resto / `q` aborta).
      `--dry-run` só imprime o plano; `--yes` aceita tudo (pra
      script). Sync **nunca apaga** em nenhuma ponta — só copia;
      remoção é sempre manual
- [x] `cdx device clean [--device <alias>]` — remove livros do device.
      Sem alvo abre um seletor interativo (setas/`j``k`, multi-seleção,
      Enter confirma) listando os livros do device; `--all` limpa tudo.
      Apaga o arquivo no device e remove a entrada correspondente de
      `device_books` (sync state). **Nunca toca no catálogo local** — a
      remoção é só na ponta do device, materializando o "remoção é
      sempre manual" deste milestone. Sempre confirma antes de apagar;
      `--yes` pula a confirmação (script). `--json` resume o que foi
      removido (path + bytes liberados)
- [x] TUI: ação de limpeza na visão do device — Space marca livros,
      confirma e apaga (espelha `cdx device clean`); a navegação já
      resolve a escolha de device sem flag
- [x] TUI: tela "Devices" — lista devices (alias, conectado ou não);
      `r` renomeia o apelido; Enter abre a visão de livros do device
      selecionado (a navegação resolve a escolha de device sem flag)
- [x] Device atual: ponteiro por catálogo (chave em `settings`) que vira o
      alvo `--device` implícito. Vira atual quando só há um device conectado
      e quando um device é escolhido explicitamente (`--device` no CLI ou
      seleção na TUI), então o "último usado" persiste entre execuções mesmo
      com vários conectados. `cdx device ls` (humano + JSONL) e a lista de
      devices da TUI marcam o atual; `resolve_target` usa o atual antes de
      cair no caso ambíguo
- [x] TUI: indicadores de presença na visão do device e na Library
      (quando há device conectado): cada linha marca "both" /
      "local only" / "device only" / "modified" via sync state +
      match normalizado, exibindo o formato de cada ponta quando
      difere
- [x] TUI: fluxo de sync espelhando o CLI iterativo — o plano vira
      lista com checkbox por item (Space marca/desmarca, `a` tudo),
      conflitos destacados exigem escolha explícita, Enter aplica só
      o que está marcado, progresso linha a linha
- [x] TUI: registrar `:devices` no command palette + ativar link
      "Devices" na welcome
- [x] TUI: push da Library — `p` na tabela (e item "Push to device" no
      menu de ações) copia o livro selecionado pro device atual após
      confirmação, reusando `cdx push`; a navegação resolve a escolha de
      device sem flag e o cabeçalho mostra o device atual conectado
      (alias + ●)

## v0.5 — Curadoria: duplicatas

Detecção de livros duplicados no catálogo atual e sugestão de qual
cópia remover. Os sinais de duplicata são combinados por **união**: se
*qualquer* método aponta suspeita, o grupo entra como candidato.

1. **Hash de conteúdo** — `book_hashes` (SHA-256 `full`/`content`):
   pega cópias byte-idênticas e o mesmo EPUB antes/depois do embed.
2. **Título + autor normalizado** — casefold + NFKD, pontuação/whitespace
   colapsados (mesma normalização do match de device): pega o mesmo
   livro em formatos/edições diferentes (EPUB vs PDF), onde o hash não
   casa.

Para cada grupo, o cdx **sugere apagar** a cópia "pior": menos metadados
preenchidos (score por presença de author/description/isbn/publisher/
language/published_date/series/tags/rating) e, no desempate, a mais
"desatualizada" (mais antiga por `added_at` / embed_status mais fraco).
A decisão final é sempre do usuário — o cdx só sugere.

- [x] `cdx dedup` — lista os grupos de duplicatas detectadas (qualquer
      método), marcando em cada grupo a cópia sugerida pra remoção e o
      motivo ("hash idêntico" / "menos metadados" / "mais antiga");
      humano + `--json` (JSONL, um objeto por grupo)
- [x] Flag `--by hash|meta|all` (default `all` = união dos sinais) pra
      restringir o método de detecção
- [x] Score de completude de metadados — função pura sobre `Book` que
      pontua a presença dos campos; elege a cópia sugerida e aparece no
      `--json`. Backfill de fingerprints garante que o método de hash
      funcione nos livros antigos
- [x] Remoção assistida — seletor (setas/`j``k` + Enter, ou aceita a
      sugestão) que reusa o caminho do `cdx rm` (apaga do catálogo +
      arquivo; `--keep` move pra cwd); `--yes` aceita todas as sugestões
      (script). Nunca apaga sem confirmação
- [x] TUI: tela/ação "Duplicates" na seção Library — lista os grupos,
      destaca a sugestão e apaga com confirmação (espelha `cdx dedup`)

## v0.6 — Conversão de formatos

- [ ] `cdx convert <id> --to epub|mobi|azw3` (delegando pra
      `ebook-convert` do Calibre se disponível)
- [ ] Detectar ausência da dependência externa com mensagem clara

## v0.7 — Outros ereaders

- [ ] Suporte a Kobo (estrutura de pastas, DB local)
- [ ] Abstração de "device driver" pra facilitar PocketBook/Boox no
      futuro

## v0.8 — Import / interop

- [ ] `cdx import calibre <path>` — importa de uma library Calibre
      existente (lê `metadata.db`)
- [ ] Export de catálogo cdx em formato neutro (JSON/CSV)

## v0.9 — TUI leitor (EPUB + TXT/Markdown)

Leitura de livros direto no terminal — única feature TUI-only do
roadmap (cf. exceção declarada no princípio de paridade). As demais
telas da TUI ficam distribuídas pelos milestones anteriores, junto
com seus comandos CLI. Sem comando CLI equivalente: o leitor é
TUI-only por design.

- [x] Renderização de EPUB — extração do spine via módulo `src/epub`
      (estende o que já existia em `src/import/epub.rs`) + HTML→texto
      via `html2text`. Reflow recomputado on resize.
- [x] Renderização de TXT/Markdown — `cdx add` aceita `.txt` e `.md`;
      Markdown via `pulldown-cmark`; TXT por leitura direta.
- [x] Paginação por altura do viewport — `:N` salta pela página
      absoluta do livro; `:cN` salta para o capítulo N. Footer mostra
      `cap X/Y · pág A/B`.
- [x] Cursor visual estilo vim (`h j k l w b e 0 $ gg G`),
      paginação (`Space`, `Ctrl+f`, `Ctrl+b`, `Ctrl+d`, `Ctrl+u`),
      troca de capítulo (`]`, `[`). `Esc` volta para a Library.
- [x] Persistir progresso de leitura — migration `0006_reading_progress`
      grava `last_chapter`, `last_offset`, `last_read_at` em `books`.
      Salva ao trocar de capítulo, paginar e ao sair do leitor.
- [x] Navegação por capítulos — `[`/`]` entre capítulos; `:cN` salta
      direto. TOC do EPUB (NCX ou nav.xhtml) usada para nomear os
      capítulos quando disponível.
- [x] `?` abre help contextual com atalhos de teclado da tela ativa.

Fora do escopo desta entrega (defer):

- Seleção visual (`v`), busca (`/`, `n`, `N`), bookmarks.
- TOC modal navegável (lista atual fica embutida na footer/help).
- Imagens inline (Kitty/Sixel) — depende de detecção de terminal.
- Exibir `last_read_at` em `cdx ls` / `cdx inspect`.

## v0.9.1 — Leitor: Kindle (MOBI/AZW3)

Estende o leitor para o ecossistema Kindle. `cdx add` já aceita
MOBI/AZW3; falta só o caminho de leitura no reader.

- [x] Reader para MOBI via crate `mobi` (`content_as_string()`, com
      fallback lossy pra livros CP1252); reaproveita o pipeline
      `html2text` → `layout` da v0.9.
- [x] Reader para AZW3 (KF8) — o container traz dois streams (MOBI
      legado KF7 + KF8). O crate `mobi` **não** parseia KF8: AZW3
      dual-stream é lido pelo stream legado; KF8-only (saída típica do
      Calibre) falha com mensagem clara sugerindo conversão pra EPUB.
- [x] Detectar DRM (Amazon Topaz / KFX / AZW protegido) com mensagem
      clara — **o cdx não remove DRM**. Só funcionam livros sideloaded
      sem DRM.
- [x] Capítulos para MOBI/AZW3 — o crate não expõe o índice (INDX),
      então o split é nos marcadores `<mbp:pagebreak/>` do MOBI6
      (determinístico, títulos "Chapter N"); sem marcadores o livro
      vira um único capítulo.

Sub-formatos validados (limitações do crate `mobi` 0.8):

- MOBI6 PalmDOC/sem compressão → lê normalmente.
- HUFF/CDIC → recusado com mensagem clara (decoder do crate não é
  confiável; preferimos recusar a renderizar livro em branco).
- AZW3 KF8-only → recusado com mensagem clara ("convert it to EPUB").
- Topaz / KFX → detectados por magic bytes antes do parse, recusados.
- Arquivo malformado/truncado → o parser do crate pode panicar; o
  reader captura via `catch_unwind` e devolve erro normal em vez de
  derrubar a sessão da TUI.

## v0.9.2 — Leitor: PDF

PDF é layout-fixo, fundamentalmente hostil ao reflow do terminal.

- [x] Reader para PDF single-column via `pdf-extract` (texto sequencial
      reaproveitado pelo `layout::lay_out`). Aceitável para a maioria
      de livros de ficção exportados em PDF. Cada página do PDF vira um
      capítulo ("Page N"), então `:cN` salta pela página real do
      documento. PDF criptografado é recusado com mensagem clara.
- [x] Heurística para detectar multi-coluna (gaps verticais em colunas
      separadas) — em texto multi-coluna o `pdf-extract` mistura linhas
      entre colunas. Sinalizar como "best-effort: layout não preservado"
      e seguir mesmo assim, ou pedir conversão para EPUB. Implementado o
      ramo "seguir mesmo assim": linha de aviso em itálico no topo de
      cada página afetada.
- [x] Tabelas, fórmulas matemáticas, imagens vetoriais — ficam
      degradadas. Documentar como limitação. Documentado no próprio
      leitor: o aviso best-effort e as mensagens de erro (criptografado,
      sem texto extraível) carregam a limitação até o usuário.
- [x] **Não usar `pdfium-render`**: exige runtime Pdfium em C++, o que
      quebra a portabilidade "binário único" do cdx. `lopdf` (já dep)
      é só para metadados; para texto, `pdf-extract` é o caminho.

## v0.9.3 — Leitor: cache de conversão e abertura assíncrona

A conversão (PDF principalmente) é cara; reabrir um livro não deve
pagar esse custo de novo, nem congelar a TUI na primeira vez.

- [x] Cache em disco do resultado da conversão (PDF/EPUB/MOBI/AZW3)
      no XDG cache dir (`~/.cache/cdx/<hash-do-catálogo>/<id>.json`);
      invalidação por mtime + tamanho do arquivo fonte + versão de
      schema. Falha de cache nunca quebra a abertura — fallback
      silencioso para a conversão. TXT/MD ficam de fora (parse é tão
      rápido quanto ler o cache).
- [x] Conversão roda em thread de fundo com tela de loading animada
      ("Opening <título>…"); a TUI continua responsiva e `Esc`
      cancela a abertura voltando pra biblioteca.

## v0.10 — Anotações e marcações

Highlights, notas e bookmarks como dado de primeira classe no
catálogo: importados do Kindle e/ou criados no leitor da TUI. Retoma
a seleção visual (`v`) e os bookmarks que a v0.9 deixou em defer.

- [ ] Migration `0008_annotations.sql` — tabela `annotations`
      (`book_id`, `kind` highlight|note|bookmark, `chapter`, `offset`,
      `text` trecho marcado, `note` comentário opcional, `source`
      kindle|cdx, `created_at`); índice por `book_id`.
- [ ] `cdx import clippings <path>` — parseia o `My Clippings.txt`
      (registros delimitados por `==========`: título/autor, tipo,
      localização, timestamp, texto) e importa todas as anotações para
      o DB, casando cada uma com o livro do catálogo por título/autor
      (não-casadas viram aviso, não erro). `source = kindle`. `--json`
      resume o que entrou. TUI: fluxo de import equivalente.
- [ ] `cdx annotations ls <id|título>` — lista anotações de um livro
      (humano + `--json`); flag `--source kindle|cdx` filtra a origem.
- [ ] TUI leitor: criar marcação via seleção visual (`v` + movimento,
      Enter confirma) e nota (input de comentário sobre o trecho
      selecionado) — persiste com `source = cdx`.
- [ ] TUI leitor: navegar anotações — lista/modal das marcações do
      livro com salto pro trecho correspondente; teclas de pular entre
      marcações documentadas no `?`.
- [ ] TUI leitor: destacar visualmente a origem — marcações importadas
      do Kindle e marcações criadas no codex usam estilos distintos
      (via `src/reader/style.rs`).
- [ ] Export de anotações em formato neutro (Markdown/JSON), agrupado
      por livro e separando origem Kindle vs codex.

Exploração (best-effort, pode escorregar pra backlog):

- [ ] Tentar reexportar pro Kindle as anotações criadas só no codex,
      reusando código opensource (plugins do Calibre, parsers de
      sidecar `.sdr`/`.pds`/`.mbp`). Formato proprietário, amarrado a
      ASIN/checksum do arquivo e instável entre firmwares — sem
      garantia de round-trip. Documentar até onde dá pra ir.

## v0.11 — Navegação por grupos (browse)

Navegar o catálogo como se fosse uma árvore de pastas: um campo de
metadado vira o "agrupador" e cada valor distinto vira uma pasta. É um
**modo da própria tela Library** (não uma seção nova) — dentro de uma
pasta valem as mesmas colunas e as mesmas ações da listagem normal.

O escopo de uma pasta é **igualdade exata** (a pasta "Jane Austen"
contém só `author = 'Jane Austen'`), diferente do filtro de busca, que é
substring. `author` é coluna única (um livro cai em uma pasta);
`tags` é many-to-many (um livro aparece em várias pastas).

- [x] `cdx groups --by author|tag|rating` — lista os grupos do catálogo
      atual (valor + contagem de livros), humano e `--json` (JSONL, um
      objeto por grupo; `value: null` no grupo catch-all — sem autor /
      sem tags / sem rating). Catálogo vazio em `--json` não imprime nada.
- [x] TUI: modo agrupado na Library — `g` abre o seletor de agrupador
      (Author / Tags / Rating / Off); o nível de "pastas" lista valor +
      contagem (`↑↓`/`jk` navegam, Enter entra). O módulo de domínio
      (`catalog::groups`) é compartilhado com o CLI.
- [x] TUI: dentro de uma pasta a tabela reusa as colunas e ações da
      listagem (inspect/edit/open/push/delete/columns/embed); um
      breadcrumb no cabeçalho mostra o grupo atual e a contagem. `Esc`
      desce uma camada por vez: limpa o filtro → volta às pastas →
      sai do agrupamento → volta à welcome.
- [x] TUI: `/` dentro de uma pasta filtra os livros do grupo em memória,
      sem alargar o escopo exato da pasta.
- [ ] (follow-up) agrupar por `publisher`/`language` — exige estender
      `SearchFilters`/`FilterCriteria`/`handle_search`; fora desta entrega.

## v1.0 — Estável

- [ ] Man page (`cdx.1`)
- [ ] Shell completions (bash/zsh/fish) — inclui completion **dinâmica**
      de argumentos posicionais (`cdx inspect <TAB>`, `cdx rm <TAB>`)
      consultando o catálogo via `clap_complete::engine::ArgValueCompleter`
- [ ] Pacote `cargo install codex` publicado no crates.io
- [ ] CI: testes + clippy + fmt
- [ ] Cobertura mínima de testes de integração

## Backlog (sem milestone)

- Servidor HTTP read-only pra browsear o catálogo de outro dispositivo
- Sync via Wi-Fi (sem cabo)
- News download / RSS-to-EPUB (à la Calibre recipes)
- Plugin system
