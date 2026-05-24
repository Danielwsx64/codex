use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::catalog::books::Book;
use crate::import::Format;

pub mod epub;
pub mod pdf;

#[derive(Debug)]
pub enum EmbedOutcome {
    Written,
    Unsupported { format: Format },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error on `{}`: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid epub `{}`: {reason}", .path.display())]
    InvalidEpub { path: PathBuf, reason: String },
    #[error("failed to parse opf from `{}`: {source}", .path.display())]
    Xml {
        path: PathBuf,
        #[source]
        source: quick_xml::Error,
    },
    #[error("zip error on `{}`: {source}", .path.display())]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("pdf error on `{}`: {source}", .path.display())]
    Pdf {
        path: PathBuf,
        #[source]
        source: lopdf::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn embed_into_file(path: &Path, format: Format, book: &Book) -> Result<EmbedOutcome> {
    match format {
        Format::Epub => {
            epub::write(path, book)?;
            Ok(EmbedOutcome::Written)
        }
        Format::Pdf => {
            pdf::write(path, book)?;
            Ok(EmbedOutcome::Written)
        }
        Format::Mobi | Format::Azw3 => Ok(EmbedOutcome::Unsupported { format }),
    }
}

pub(crate) fn write_atomic<F>(path: &Path, fill: F) -> Result<()>
where
    F: FnOnce(&mut File) -> Result<()>,
{
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp_name = format!(
        ".{}.cdx.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("embed")
    );
    let tmp = parent.join(tmp_name);
    {
        let mut file = File::create(&tmp).map_err(|source| Error::Io {
            path: tmp.clone(),
            source,
        })?;
        match fill(&mut file) {
            Ok(()) => {}
            Err(err) => {
                drop(file);
                let _ = fs::remove_file(&tmp);
                return Err(err);
            }
        }
        file.flush().map_err(|source| Error::Io {
            path: tmp.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| Error::Io {
            path: tmp.clone(),
            source,
        })?;
    }
    fs::rename(&tmp, path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}
