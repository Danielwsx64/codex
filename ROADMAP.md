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

Exceções (raras, sempre justificadas):

- Setup imperativo (`cdx catalog init/add/use/rm`) — one-shots de
  configuração do registro de catálogos; uma tela TUI adicionaria
  fricção pra fluxos que são essencialmente "registrar path / trocar
  atual". A tela "Catalogs" pode entrar como startup da TUI quando
  fizer sentido (>1 catálogo registrado).
- Leitor de livros (v0.8) — só faz sentido na TUI.

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

## v0.4 — Kindle sync (USB)

- [ ] Detectar Kindle montado (heurística por `system.bin` / vendor id)
- [ ] `cdx device ls` — lista livros no device
- [ ] `cdx push <id>` — copia arquivo do catálogo pro Kindle
- [ ] `cdx pull <path>` — importa livro do Kindle pro catálogo
- [ ] `cdx sync` — diff bidirecional com confirmação

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
