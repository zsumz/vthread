//! Owned filesystem requests delegated to the bounded native pool.
//!
//! Cancelling a caller cannot undo native I/O already in progress. Writes may have
//! committed when cancellation is observed; these helpers are not transactions.

use crate::{Error, Result, SuspensionReason, blocking};
use std::{io::Read, path::Path};

/// Reads at most `limit` bytes, returning an error if the file is larger.
/// Storage grows only for bytes actually read, with fallible reservations bounded by
/// `limit`. A fixed 8 KiB scratch buffer detects overflow without allocating the limit.
pub fn read(path: impl AsRef<Path>, limit: usize) -> Result<Vec<u8>> {
    let path = path.as_ref().to_owned();
    blocking::run_for(SuspensionReason::FileIo, move || {
        let mut file = std::fs::File::open(&path)
            .map_err(|error| Error::io("open for read", path.display(), error))?;
        let mut data = Vec::new();
        let mut scratch = [0; 8192];
        loop {
            let remaining = limit - data.len();
            let probe = remaining.saturating_add(1).min(scratch.len());
            match file.read(&mut scratch[..probe]) {
                Ok(0) => break,
                Ok(count) => {
                    if count > remaining {
                        return Err(Error::LimitExceeded {
                            resource: "file bytes",
                            limit,
                        });
                    }
                    let needed = data.len() + count;
                    if needed > data.capacity() {
                        let capacity = needed.max(data.capacity().saturating_mul(2)).min(limit);
                        data.try_reserve_exact(capacity - data.len()).map_err(|_| {
                            Error::AllocationFailed {
                                resource: "file bytes",
                                requested: capacity,
                            }
                        })?;
                    }
                    data.extend_from_slice(&scratch[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(Error::io("read", path.display(), error)),
            }
        }
        Ok(data)
    })?
}

/// Writes owned bytes on a worker. Cancellation does not roll back side effects.
pub fn write(path: impl AsRef<Path>, data: Vec<u8>) -> Result<()> {
    let path = path.as_ref().to_owned();
    blocking::run_for(SuspensionReason::FileIo, move || {
        std::fs::write(&path, data).map_err(|error| Error::io("write", path.display(), error))
    })??;
    Ok(())
}

/// Reads filesystem metadata on a worker.
pub fn metadata(path: impl AsRef<Path>) -> Result<std::fs::Metadata> {
    let path = path.as_ref().to_owned();
    blocking::run_for(SuspensionReason::FileIo, move || {
        std::fs::metadata(&path).map_err(|error| Error::io("metadata", path.display(), error))
    })?
}

#[cfg(test)]
#[path = "fs_test.rs"]
mod fs_test;
