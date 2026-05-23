use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

pub mod catalog;

#[derive(Parser, Debug)]
#[command(
    name = "cdx",
    version,
    about = "Terminal-first ebook library and ereader manager"
)]
pub struct Cli {
    #[arg(long, global = true, help = "Emit machine-readable JSONL on stdout")]
    pub json: bool,

    #[arg(
        short,
        long,
        global = true,
        action = ArgAction::Count,
        help = "Increase log verbosity (-v info, -vv debug, -vvv trace)"
    )]
    pub verbose: u8,

    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Override the cdx config dir (defaults to $XDG_CONFIG_HOME/cdx); intended for tests"
    )]
    pub data_dir: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        value_name = "NAME",
        help = "Use a registered catalog other than the current one for this invocation"
    )]
    pub catalog: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "Open the cdx terminal UI")]
    Tui,
    #[command(subcommand, about = "Manage cdx catalogs (libraries)")]
    Catalog(CatalogCmd),
}

#[derive(Subcommand, Debug)]
pub enum CatalogCmd {
    #[command(about = "Create a new catalog at PATH and register it under NAME")]
    Init {
        name: String,
        path: PathBuf,
        #[arg(long, value_name = "TEXT", help = "Short description for the catalog")]
        description: Option<String>,
        #[arg(
            long,
            help = "Do not switch the current catalog to the new one (still set if no current exists)"
        )]
        no_switch: bool,
    },
    #[command(about = "Register an existing catalog directory under NAME")]
    Add {
        name: String,
        path: PathBuf,
        #[arg(long, value_name = "TEXT", help = "Short description for the catalog")]
        description: Option<String>,
        #[arg(long, help = "Do not switch the current catalog to the registered one")]
        no_switch: bool,
    },
    #[command(about = "List all registered catalogs")]
    Ls,
    #[command(about = "Switch the current catalog to NAME")]
    Use { name: String },
    #[command(about = "Unregister a catalog; optionally delete its files from disk")]
    Rm {
        name: String,
        #[arg(long, help = "Also delete the catalog directory and all its files")]
        purge: bool,
    },
}
