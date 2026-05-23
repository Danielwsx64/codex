# Roadmap

Formato: milestones versionados. Cada release é um escopo fechado;
quando todos os checks estão marcados, lança-se a versão e abre-se a
próxima.

## v0.1 — MVP catálogo

Catálogo local independente, sem device sync, sem Calibre.

- [ ] Definir esquema do catálogo (SQLite local em `$XDG_DATA_HOME/cdx`)
- [ ] `cdx init` — cria diretório de dados e DB
- [ ] `cdx add <file>` — importa EPUB/PDF/MOBI, extrai metadados básicos
- [ ] `cdx ls` — lista livros (id, título, autor)
- [ ] `cdx show <id>` — exibe metadados detalhados
- [ ] `cdx rm <id>` — remove do catálogo (com flag pra apagar arquivo)
- [ ] Logging configurável via `RUST_LOG`

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
- TUI mode (ratatui)
