use vox_esp32_core::Controller;

pub async fn run(controller: &mut Controller) -> anyhow::Result<()> {
    log::info!("Started CAN test ...");

    let can_rx = vox_esp32_power::can::register_handlers(controller)?;

    let _handle = controller.start_can()?;

    loop {
        match can_rx.recv().await {
            Ok((msg_id, message_type, message)) => {
                log::info!(
                    "Received CAN Msg: msg_id={:?}, message_type={:?}, message={:?}",
                    msg_id,
                    message_type,
                    message
                );
            }
            Err(e) => {
                log::error!("Failed to receive CAN message: {}", e);
            }
        }
    }
}
