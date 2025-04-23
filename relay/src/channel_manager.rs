use anyhow::{anyhow, bail, Result};
use std::sync::{Arc, Mutex};

pub(crate) struct BiParentComm<ParentToChildMsg, ChildToParentMsg>
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    parent_to_child: crossbeam::channel::Sender<ParentToChildMsg>,
    child_to_parent: crossbeam::channel::Receiver<ChildToParentMsg>,
    killswitch: Arc<Mutex<bool>>,
}

pub(crate) struct BiChildComm<ParentToChildMsg, ChildToParentMsg>
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    parent_to_child: crossbeam::channel::Receiver<ParentToChildMsg>,
    child_to_parent: crossbeam::channel::Sender<ChildToParentMsg>,
    killswitch: Arc<Mutex<bool>>,
}

pub(crate) fn create_bichannels<ParentToChildMsg, ChildToParentMsg>() -> (
    BiParentComm<ParentToChildMsg, ChildToParentMsg>,
    BiChildComm<ParentToChildMsg, ChildToParentMsg>,
)
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    let killswitch = Arc::new(Mutex::from(false));
    let killswitch_clone = killswitch.clone();

    let (tx_to_child, rx_from_parent) = crossbeam::channel::unbounded();
    let (tx_to_parent, rx_from_child) = crossbeam::channel::unbounded();

    let parent_comm: BiParentComm<ParentToChildMsg, ChildToParentMsg> = BiParentComm {
        parent_to_child: tx_to_child,
        child_to_parent: rx_from_child,
        killswitch,
    };

    let child_comm: BiChildComm<ParentToChildMsg, ChildToParentMsg> = BiChildComm {
        parent_to_child: rx_from_parent,
        child_to_parent: tx_to_parent,
        killswitch: killswitch_clone,
    };

    (parent_comm, child_comm)
}

impl<ParentToChildMsg, ChildToParentMsg> BiParentComm<ParentToChildMsg, ChildToParentMsg>
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    pub fn killswitch_engage(&self) -> Result<()> {
        let Ok(mut killswitch) = self.killswitch.lock() else {
            bail!("Failed to acquire mutex lock.")
        };

        *killswitch = true;

        Ok(())
    }

    pub fn send_to_child(&mut self, msg: ParentToChildMsg) -> Result<()> {
        self.parent_to_child
            .send(msg)
            .map_err(|e| anyhow!("Converted crossbeam error: {}", e.to_string()))
    }

    pub fn recv_messages_iter(&self) -> Vec<ChildToParentMsg> {
        self.child_to_parent.try_iter().collect()
    }
}

impl<ParentToChildMsg, ChildToParentMsg> BiChildComm<ParentToChildMsg, ChildToParentMsg>
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    pub fn is_killswitch(&self) -> bool {
        let Ok(killswitch) = self.killswitch.lock() else {
            return true;
        };

        *killswitch
    }

    pub fn send_to_parent(&mut self, msg: ChildToParentMsg) -> Result<()> {
        self.child_to_parent
            .send(msg)
            .map_err(|e| anyhow!("Converted crossbeam error: {}", e.to_string()))
    }

    pub fn received_messages(&self) -> Vec<ParentToChildMsg> {
        self.parent_to_child.try_iter().collect()
    }
}
