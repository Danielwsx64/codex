use std::io;

use clap::CommandFactory;
use clap_complete::Shell;

use crate::cli::Cli;

pub fn dispatch(shell: Shell) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "cdx", &mut io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_tree_is_valid_for_completion() {
        // `generate` walks the whole command tree; a malformed clap definition
        // panics here, so this doubles as a structural sanity check.
        let mut cmd = Cli::command();
        let mut out = Vec::new();
        clap_complete::generate(Shell::Bash, &mut cmd, "cdx", &mut out);
        assert!(!out.is_empty());
        assert!(String::from_utf8_lossy(&out).contains("cdx"));
    }
}
