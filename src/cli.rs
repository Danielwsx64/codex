use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

pub mod books;
pub mod catalog;
pub mod edit;
pub mod embed;

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
    #[command(
        about = "Import one or more ebook files (epub, pdf, mobi, azw3) into the current catalog"
    )]
    Add {
        #[arg(required = true, value_name = "PATH", num_args = 1.., help = "One or more ebook files to import")]
        paths: Vec<PathBuf>,
        #[arg(
            long,
            help = "Import even if a file with the same content is already in the catalog"
        )]
        force: bool,
    },
    #[command(about = "List books in the current catalog")]
    Ls {
        #[arg(
            long,
            value_name = "LIST",
            conflicts_with = "all_columns",
            help = "Comma-separated column slugs to show, in display order (use --all-columns to list every slug)"
        )]
        columns: Option<String>,
        #[arg(
            long,
            help = "Show every available column (id, title, author, tags, series, rating, publisher, language, published, isbn, format, embed)"
        )]
        all_columns: bool,
    },
    #[command(about = "Edit a book's metadata in $EDITOR (TOML)")]
    Edit {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive)"
        )]
        target: String,
    },
    #[command(about = "Show metadata for a book (by numeric id or title)")]
    Inspect {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive)"
        )]
        target: String,
    },
    #[command(
        about = "Search books by substring across title, author and tags (whitespace = AND tokens)"
    )]
    Search {
        #[arg(
            value_name = "QUERY",
            help = "Search query; multiple whitespace-separated tokens must all match"
        )]
        query: String,
    },
    #[command(about = "Add tags to a book")]
    Tag {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive)"
        )]
        target: String,
        #[arg(
            required = true,
            num_args = 1..,
            value_name = "TAG",
            help = "One or more tag names to add (case-insensitive match against existing tags)"
        )]
        tags: Vec<String>,
    },
    #[command(about = "Set or clear a book's rating (0 clears)")]
    Rate {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive)"
        )]
        target: String,
        #[arg(
            value_name = "RATING",
            value_parser = clap::value_parser!(u8).range(0..=5),
            help = "Star rating from 0 (clears) to 5"
        )]
        rating: u8,
    },
    #[command(about = "Set or clear a book's series")]
    Series {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive)"
        )]
        target: String,
        #[arg(
            value_name = "NAME",
            required_unless_present = "clear",
            conflicts_with = "clear",
            help = "Series name to assign"
        )]
        name: Option<String>,
        #[arg(
            long,
            value_name = "N",
            conflicts_with = "clear",
            help = "Position in the series (decimal); omitted preserves the current index"
        )]
        index: Option<f64>,
        #[arg(long, help = "Remove the series (and its index) from the book")]
        clear: bool,
    },
    #[command(about = "Remove tags from a book")]
    Untag {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive)"
        )]
        target: String,
        #[arg(
            num_args = 0..,
            value_name = "TAG",
            conflicts_with = "all",
            help = "One or more tag names to remove (case-insensitive)"
        )]
        tags: Vec<String>,
        #[arg(long, help = "Remove every tag currently on the book")]
        all: bool,
    },
    #[command(about = "Remove a book from the catalog; by default deletes its file")]
    Rm {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive)"
        )]
        target: String,
        #[arg(
            long,
            help = "Move the book file to the current working directory instead of deleting it"
        )]
        keep: bool,
    },
    #[command(subcommand, about = "Manage metadata embedded into book files")]
    Embed(EmbedCmd),
}

#[derive(Subcommand, Debug)]
pub enum EmbedCmd {
    #[command(about = "Embed catalog metadata into every book with status `pending`")]
    Sync,
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
