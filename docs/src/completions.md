# Shell completions

`cdx completions <shell>` prints a completion script to stdout. Supported shells:
`bash`, `zsh`, `fish`, `elvish`, `powershell`.

## bash

```sh
# system-wide (needs bash-completion installed)
cdx completions bash | sudo tee /etc/bash_completion.d/cdx >/dev/null

# or per-user: source it from ~/.bashrc
cdx completions bash > ~/.local/share/bash-completion/completions/cdx
```

## zsh

```sh
# put it on your $fpath, e.g.
cdx completions zsh > ~/.zfunc/_cdx
# then ensure ~/.zfunc is on the fpath before compinit in ~/.zshrc:
#   fpath=(~/.zfunc $fpath)
#   autoload -U compinit && compinit
```

## fish

```sh
cdx completions fish > ~/.config/fish/completions/cdx.fish
```

Reload your shell (or `source` the relevant rc file) and completions for `cdx`
subcommands and flags become available.

> Dynamic completion of positional arguments (for example, completing book titles
> and ids in `cdx inspect <TAB>` against the catalog) is planned for a later
> release; today's completions cover subcommands and flags.
