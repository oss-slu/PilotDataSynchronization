use anyhow::{anyhow, bail, Result};
use std::sync::{Arc, Mutex};

pub(crate) struct ParentBiChannel<ParentToChildMsg, ChildToParentMsg>
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    parent_to_child: crossbeam::channel::Sender<ParentToChildMsg>,
    child_to_parent: crossbeam::channel::Receiver<ChildToParentMsg>,
    killswitch: Arc<Mutex<bool>>,
    connected_to_endpoint: Arc<Mutex<bool>>,
}

pub(crate) struct ChildBiChannel<ParentToChildMsg, ChildToParentMsg>
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    parent_to_child: crossbeam::channel::Receiver<ParentToChildMsg>,
    child_to_parent: crossbeam::channel::Sender<ChildToParentMsg>,
    killswitch: Arc<Mutex<bool>>,
    connected_to_endpoint: Arc<Mutex<bool>>,
}

pub(crate) fn create_bichannels<ParentToChildMsg, ChildToParentMsg>() -> (
    ParentBiChannel<ParentToChildMsg, ChildToParentMsg>,
    ChildBiChannel<ParentToChildMsg, ChildToParentMsg>,
)
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    let killswitch = Arc::new(Mutex::from(false));
    let killswitch_clone = killswitch.clone();

    let connected_to_endpoint = Arc::new(Mutex::from(false));
    let connected_to_endpoint_clone = connected_to_endpoint.clone();

    let (tx_to_child, rx_from_parent) = crossbeam::channel::unbounded();
    let (tx_to_parent, rx_from_child) = crossbeam::channel::unbounded();

    let parent_comm: ParentBiChannel<ParentToChildMsg, ChildToParentMsg> = ParentBiChannel {
        parent_to_child: tx_to_child,
        child_to_parent: rx_from_child,
        killswitch,
        connected_to_endpoint,
    };

    let child_comm: ChildBiChannel<ParentToChildMsg, ChildToParentMsg> = ChildBiChannel {
        parent_to_child: rx_from_parent,
        child_to_parent: tx_to_parent,
        killswitch: killswitch_clone,
        connected_to_endpoint: connected_to_endpoint_clone,
    };

    (parent_comm, child_comm)
}

impl<ParentToChildMsg, ChildToParentMsg> ParentBiChannel<ParentToChildMsg, ChildToParentMsg>
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

    pub fn received_messages(&self) -> Vec<ChildToParentMsg> {
        self.child_to_parent.try_iter().collect()
    }

    pub fn _try_recv(&self) -> Result<ChildToParentMsg> {
        self.child_to_parent
            .try_recv()
            .map_err(|e| anyhow!("Converted crossbeam error: {}", e.to_string()))
    }

    pub fn is_conn_to_endpoint(&self) -> Result<bool> {
        let Ok(conn_status) = self.connected_to_endpoint.lock() else {
            bail!("Failed to acquire mutex lock.")
        };

        Ok(*conn_status)
    }
}

impl<ParentToChildMsg, ChildToParentMsg> ChildBiChannel<ParentToChildMsg, ChildToParentMsg>
where
    ParentToChildMsg: Send + Sync,
    ChildToParentMsg: Send + Sync,
{
    pub fn is_killswitch_engaged(&self) -> bool {
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

    pub fn set_is_conn_to_endpoint(&mut self, new_conn_status: bool) -> Result<()> {
        let Ok(mut conn_status) = self.connected_to_endpoint.lock() else {
            bail!("Failed to acquire mutex lock.")
        };

        *conn_status = new_conn_status;

        Ok(())
    }
}
