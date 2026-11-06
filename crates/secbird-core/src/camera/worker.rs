//! Plain-value request boundary; no COM objects cross threads.
use crate::{Error, Result};
use std::sync::mpsc::{self, Sender, SyncSender, TrySendError};

pub(super) fn call<T, R>(
    sender: &SyncSender<R>,
    make: impl FnOnce(Sender<Result<T>>) -> R,
) -> Result<T> {
    let (reply, receive) = mpsc::channel();
    sender.try_send(make(reply)).map_err(|error| match error {
        TrySendError::Full(_) => Error::System("camera worker busy; try again".into()),
        TrySendError::Disconnected(_) => Error::System("camera worker disconnected".into()),
    })?;
    receive
        .recv()
        .map_err(|_| Error::System("camera worker stopped before replying".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_worker_handles_closed_full_and_lost_reply() {
        let (tx, rx) = mpsc::sync_channel::<Sender<Result<()>>>(1);
        tx.send(mpsc::channel().0).unwrap();
        assert!(call(&tx, |reply| reply)
            .unwrap_err()
            .to_string()
            .contains("busy"));
        drop(rx);
        assert!(call(&tx, |reply| reply)
            .unwrap_err()
            .to_string()
            .contains("disconnected"));
        let (tx, rx) = mpsc::sync_channel::<Sender<Result<()>>>(1);
        let thread = std::thread::spawn(move || {
            drop(rx.recv().unwrap());
        });
        assert!(call(&tx, |reply| reply)
            .unwrap_err()
            .to_string()
            .contains("replying"));
        thread.join().unwrap();
    }
}
