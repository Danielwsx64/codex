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
- [ ] `cdx add <file>` — importa EPUB/PDF/MOBI, extrai metadados básicos
- [ ] `cdx ls` — lista livros (id, título, autor)
- [ ] `cdx show <id>` — exibe metadados detalhados
- [ ] `cdx rm <id>` — remove do catálogo (com flag pra apagar arquivo)
- [x] Logging configurável via `RUST_LOG` (`tracing-subscriber` lê
      `RUST_LOG`; `-v/-vv/-vvv` ajusta o default sem precisar exportar)
- [x] Tela de boas vindas em módulo compartilhado, exibida quando `cdx`
      roda sem subcomando (mesmo conteúdo será reusado pela TUI)
- [x] `cdx tui` — esqueleto ratatui + tela de boas vindas reusando o
      módulo compartilhado (prova o ciclo CLI↔TUI; demais telas entram
      junto com seus respectivos comandos nos milestones seguintes)

## v0.2 — Edição de metadados

- [ ] `cdx edit <id>` — abre `$EDITOR` com TOML/YAML dos metadados
- [ ] `cdx tag <id> <tag>...` / `cdx untag`
- [ ] `cdx rate <id> <0-5>`
- [ ] `cdx series <id> <name> [--index N]`

## v0.3 — Busca e filtros

- [ ] `cdx search <query>` — full-text em título/autor/tags
- [ ] Flags `--author`, `--tag`, `--series`, `--rating`
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
- [ ] `?` abre help contextual com atalhos de teclado da tela ativa

## v1.0 — Estável

- [ ] Man page (`cdx.1`)
- [ ] Shell completions (bash/zsh/fish)
- [ ] Pacote `cargo install codex` publicado no crates.io
- [ ] CI: testes + clippy + fmt
- [ ] Cobertura mínima de testes de integração

## Backlog (sem milestone)

- Servidor HTTP read-only pra browsear o catálogo de outro dispositivo
- Sync via Wi-Fi (sem cabo)
- News download / RSS-to-EPUB (à la Calibre recipes)
- Plugin system
