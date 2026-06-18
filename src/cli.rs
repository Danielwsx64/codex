use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

pub mod books;
pub mod catalog;
pub mod device;
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

// Column-selection flags shared by every command that lists books. Flatten
// this into new list-style commands instead of redeclaring the two flags.
#[derive(Args, Debug, Default)]
pub struct LibraryColumnArgs {
    #[arg(
        long,
        value_name = "LIST",
        conflicts_with = "all_columns",
        help = "Comma-separated column slugs to show, in display order (use --all-columns to list every slug)"
    )]
    pub columns: Option<String>,
    #[arg(
        long,
        help = "Show every available column (id, title, author, tags, series, rating, publisher, language, published, isbn, format, embed)"
    )]
    pub all_columns: bool,
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
        #[command(flatten)]
        view: LibraryColumnArgs,
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
    #[command(about = "Search books by substring, optionally filtered by field")]
    Search {
        #[arg(
            value_name = "QUERY",
            required_unless_present_any = ["author", "tag", "series", "rating"],
            help = "Search query across title/author/tags; whitespace-separated tokens are AND'd"
        )]
        query: Option<String>,
        #[arg(
            long,
            value_name = "AUTHOR",
            help = "Substring match on the author field (case-insensitive)"
        )]
        author: Option<String>,
        #[arg(
            long,
            value_name = "TAG",
            help = "Substring match on a tag; repeat for AND semantics"
        )]
        tag: Vec<String>,
        #[arg(
            long,
            value_name = "SERIES",
            help = "Substring match on the series name (case-insensitive)"
        )]
        series: Option<String>,
        #[arg(
            long,
            value_name = "N or MIN..MAX",
            value_parser = clap::value_parser!(crate::catalog::books::RatingRange),
            help = "Rating filter: exact (4) or inclusive range (3..5); both ends in 0..=5"
        )]
        rating: Option<crate::catalog::books::RatingRange>,
        #[command(flatten)]
        view: LibraryColumnArgs,
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
    #[command(about = "Copy a book from the catalog to a connected device")]
    Push {
        #[arg(
            value_name = "ID_OR_TITLE",
            help = "Numeric id, or exact title (case-insensitive). Omit to pick interactively"
        )]
        target: Option<String>,
        #[arg(
            long,
            value_name = "SERIAL_OR_ALIAS",
            help = "Target device (defaults to the only connected device)"
        )]
        device: Option<String>,
    },
    #[command(about = "Import a book from a connected device into the catalog")]
    Pull {
        #[arg(
            value_name = "PATH",
            help = "Path to the book on the device (relative to the mount, e.g. documents/Dune.epub). Omit to pick interactively"
        )]
        path: Option<String>,
        #[arg(
            long,
            value_name = "SERIAL_OR_ALIAS",
            help = "Source device (defaults to the only connected device)"
        )]
        device: Option<String>,
        #[arg(long, help = "Import even if the content is already in the catalog")]
        force: bool,
    },
    #[command(about = "Sync books between the catalog and a connected device")]
    Sync {
        #[arg(
            long,
            value_name = "SERIAL_OR_ALIAS",
            help = "Target device (defaults to the only connected device)"
        )]
        device: Option<String>,
        #[arg(long, help = "Print the sync plan without copying anything")]
        dry_run: bool,
        #[arg(long, help = "Apply every item without asking (for scripts)")]
        yes: bool,
        #[arg(
            long,
            help = "Re-hash files that pass the size+mtime check (slower, catches silent edits)"
        )]
        verify: bool,
    },
    #[command(subcommand, about = "Manage ereaders (devices)")]
    Device(DeviceCmd),
}

#[derive(Subcommand, Debug)]
pub enum DeviceCmd {
    #[command(about = "List detected and known devices")]
    Ls,
    #[command(about = "Set or rename a device alias")]
    Alias {
        #[arg(value_name = "SERIAL_OR_ALIAS")]
        target: String,
        #[arg(value_name = "NEW_ALIAS")]
        new_alias: String,
    },
    #[command(about = "List books on a connected device with catalog presence")]
    Books {
        #[arg(
            long,
            value_name = "SERIAL_OR_ALIAS",
            help = "Target device (defaults to the only connected device)"
        )]
        device: Option<String>,
    },
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
