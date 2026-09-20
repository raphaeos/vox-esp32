use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Peripheral already taken: {0}")]
    PeripheralTaken(&'static str),
    #[error("Exceeded CAN message id length")]
    CANMsgIdExceeded,
    #[error("Error: {0}")]
    Other(#[from] anyhow::Error),
}
