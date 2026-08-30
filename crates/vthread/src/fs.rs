//! Owned filesystem requests delegated to the bounded native pool.
//!
//! Cancelling a caller cannot undo native I/O already in progress. Writes may have
//! committed when cancellation is observed; these helpers are not transactions.

use crate::{Error, Result, SuspensionReason, blocking};
use std::{io::Read, path::Path};

/// Reads at most `limit` bytes, returning an error if the file is larger.
/// The result allocation is bounded by limit plus one detection byte.
pub fn read(path: impl AsRef<Path>, limit: usize) -> Result<Vec<u8>> {
    let maximum = limit.checked_add(1).ok_or(Error::invalid_configuration(
        "read_limit",
        "must be below usize::MAX",
    ))?;
    let path = path.as_ref().to_owned();
    blocking::run_for(SuspensionReason::FileIo, move || {
        let mut file = std::fs::File::open(path)?;
        let mut data = vec![0; maximum];
        let mut used = 0;
        while used < maximum {
            match file.read(&mut data[used..]) {
                Ok(0) => break,
                Ok(count) => used += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if used > limit {
            return Err(Error::LimitExceeded {
                resource: "file bytes",
                limit,
            });
        }
        data.truncate(used);
        Ok(data)
    })?
}

/// Writes owned bytes on a worker. Cancellation does not roll back side effects.
pub fn write(path: impl AsRef<Path>, data: Vec<u8>) -> Result<()> {
    let path = path.as_ref().to_owned();
    blocking::run_for(SuspensionReason::FileIo, move || std::fs::write(path, data))??;
    Ok(())
}

/// Reads filesystem metadata on a worker.
pub fn metadata(path: impl AsRef<Path>) -> Result<std::fs::Metadata> {
    let path = path.as_ref().to_owned();
    Ok(blocking::run_for(SuspensionReason::FileIo, move || {
        std::fs::metadata(path)
    })??)
}

#[cfg(test)]
#[path = "fs_test.rs"]
mod fs_test;
