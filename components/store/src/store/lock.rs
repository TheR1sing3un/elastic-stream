use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::fd::{FromRawFd, IntoRawFd, RawFd},
    path::Path,
};

use crate::error::StoreError;
use byteorder::{BigEndian, ReadBytesExt};
use client::IdGenerator;
use nix::fcntl::{flock, FlockArg};
use slog::{error, info, Logger};

pub(crate) struct Lock {
    log: Logger,
    fd: RawFd,
    id: i32,
}

impl Lock {
    pub(crate) fn new(
        store_path: &Path,
        id_generator: Box<dyn IdGenerator>,
        log: &Logger,
    ) -> Result<Self, StoreError> {
        let lock_file_path = store_path.join("LOCK");
        let (fd, id) = if lock_file_path.as_path().exists() {
            let mut file = OpenOptions::new()
                .read(true)
                .open(lock_file_path.as_path())?;
            let id = file.read_i32::<BigEndian>()?;
            (file.into_raw_fd(), id)
        } else {
            let mut file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(lock_file_path.as_path())?;

            let id: i32 = match id_generator.generate() {
                Ok(id) => id,
                Err(_e) => {
                    return Err(StoreError::Configuration(String::from(
                        "Failed to acquire data-node ID",
                    )));
                }
            };
            file.write_all(&id.to_be_bytes())?;
            file.sync_all()?;
            (file.into_raw_fd(), id)
        };

        info!(
            log,
            "Acquiring store lock: {:?}, id={}",
            lock_file_path.as_path(),
            id
        );

        flock(fd, FlockArg::LockExclusive).map_err(|e| {
            error!(log, "Failed to acquire store lock. errno={}", e);
            StoreError::AcquireLock
        })?;

        info!(log, "Store lock acquired. ID={}", id);

        Ok(Self {
            log: log.clone(),
            fd,
            id,
        })
    }

    pub(crate) fn id(&self) -> i32 {
        self.id
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Err(e) = flock(self.fd, FlockArg::Unlock) {
            error!(self.log, "Failed to release store lock. errno={}", e);
        }
        let _file = unsafe { File::from_raw_fd(self.fd) };
    }
}

#[cfg(test)]
mod tests {
    use client::PlacementManagerIdGenerator;
    use tokio::sync::oneshot;

    use super::Lock;
    use std::error::Error;

    #[test]
    fn test_lock_normal() -> Result<(), Box<dyn Error>> {
        let log = test_util::terminal_logger();
        let path = test_util::create_random_path()?;
        let _guard = test_util::DirectoryRemovalGuard::new(log.clone(), path.as_path());

        let (stop_tx, stop_rx) = oneshot::channel();
        let (port_tx, port_rx) = oneshot::channel();

        let logger = log.clone();
        let handle = std::thread::spawn(move || {
            tokio_uring::start(async {
                let port = test_util::run_listener(logger).await;
                let _ = port_tx.send(port);
                let _ = stop_rx.await;
            });
        });

        let port = port_rx.blocking_recv().unwrap();
        let pm_address = format!("localhost:{}", port);
        let generator = Box::new(PlacementManagerIdGenerator::new(
            log.clone(),
            &pm_address,
            "dn-host",
        ));

        let _lock = Lock::new(path.as_path(), generator, &log)?;
        let _ = stop_tx.send(());
        let _ = handle.join();
        Ok(())
    }
}