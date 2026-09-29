use async_channel::{bounded, Receiver, SendError, Sender, TrySendError};

pub struct LossyChannel<T> {
    tx: Sender<T>,
    rx: Receiver<T>,
}

impl<T> LossyChannel<T> {
    fn new(capacity: usize) -> Self {
        let (tx, rx) = bounded(capacity);

        Self { tx, rx }
    }

    /// Sends a message. If full, it evicts the oldest message to make room.
    pub async fn send_lossy(&self, msg: T) -> Result<(), SendError<T>> {
        while self.tx.is_full() {
            // Drop the oldest message to clear a slot
            let _ = self.rx.try_recv();
        }

        // Now that a slot is guaranteed (or the channel closed), send it
        self.tx.send(msg).await
    }

    pub fn try_send_lossy(&self, msg: T) -> Result<(), TrySendError<T>> {
        while self.tx.is_full() {
            // Drop the oldest message to clear a slot
            let _ = self.rx.try_recv();
        }

        // Now that a slot is guaranteed (or the channel closed), send it
        self.tx.try_send(msg)
    }

    pub fn receiver(&self) -> Receiver<T> {
        self.rx.clone()
    }
}

impl<T> Clone for LossyChannel<T> {
    fn clone(&self) -> Self {
        LossyChannel {
            tx: self.tx.clone(),
            rx: self.rx.clone(),
        }
    }
}

pub fn lossy_bounded<T>(capacity: usize) -> (LossyChannel<T>, Receiver<T>) {
    let tx = LossyChannel::new(capacity);
    let rx = tx.receiver().clone();

    (tx, rx)
}
