use core::time::Duration;
use iceoryx2::prelude::*;

const CYCLE_TIME: Duration = Duration::from_secs(1);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node = NodeBuilder::new().create::<ipc::Service>()?;

    // create our port factory by creating or opening the service
    let service = node
        .service_builder(&"IPC/Test".try_into()?)
        .publish_subscribe::<u64>()
        .open_or_create()?;

    let subscriber = service.subscriber_builder().create()?;

    while node.wait(CYCLE_TIME).is_ok() {
        while let Some(sample) = subscriber.receive()? {
            println!("received: {:?}", *sample);
        }
    }

    Ok(())
}
