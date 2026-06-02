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

- Leitor de livros (v0.8) — só faz sentido na TUI.

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
4. **Devices** — sync com ereaders (`cdx device ls`, `push`, `pull`,
   `sync`) [v0.4]

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
- [ ] Saída `--json` pra compor com `jq`/scripts
- [ ] TUI: registrar `:search` no command palette + ativar link
      "Search" na welcome

## v0.4 — Kindle sync (USB)

- [ ] Detectar Kindle montado (heurística por `system.bin` / vendor id)
- [ ] `cdx device ls` — lista livros no device
- [ ] `cdx push <id>` — copia arquivo do catálogo pro Kindle
- [ ] `cdx pull <path>` — importa livro do Kindle pro catálogo
- [ ] `cdx sync` — diff bidirecional com confirmação
- [ ] TUI: registrar `:devices` no command palette + ativar link
      "Devices" na welcome

## v0.5 — Conversão de formatos

- [ ] `cdx convert <id> --to epub|mobi|azw3` (delegando pra
      `ebook-convert` do Calibre se disponível)
- [ ] Detectar ausência da dependência externa com mensagem clara

## v0.6 — Outros ereaders

- [ ] Suporte a Kobo (estrutura de pastas, DB local)
- [ ] Abstração de "device driver" pra facilitar PocketBook/Boox no
      futuro

## v0.7 — Import / interop

- [ ] `cdx import calibre <path>` — importa de uma library Calibre
      existente (lê `metadata.db`)
- [ ] Export de catálogo cdx em formato neutro (JSON/CSV)

## v0.8 — TUI leitor

Leitura de livros direto no terminal — única feature TUI-only do
roadmap (cf. exceção declarada no princípio de paridade). As demais
telas da TUI ficam distribuídas pelos milestones anteriores, junto
com seus comandos CLI.

- [ ] Renderização de EPUB (texto + paginação)
- [ ] Renderização de TXT/Markdown
- [ ] Persistir progresso de leitura por livro no catálogo
- [ ] Navegação por capítulos / sumário
- [x] `?` abre help contextual com atalhos de teclado da tela ativa

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
