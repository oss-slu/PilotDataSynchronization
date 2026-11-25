# The relay outlives the listener and retries its outputs

Status: accepted. The relay's side is #276; the launchers and the listeners' resume are planned in #278 and #277.

The launchers start the **Relay** as its own process, apart from the Python listener (`data_logger` or `live_groups`). They leave it running when the listener stops, and reuse it if it is already running. Given `--tcp` or `--mirror`, the relay keeps retrying that address until it connects, and again after a drop, so a listener started again mid-**Flight** is picked up with no clicks. We chose this because the **Plugin** cannot reconnect to a relay that has restarted (#256, #255): a relay that closed with its listener would cost the **Instructor** an X-Plane restart mid-**Flight**. In the lab, closing the console window is how people stop a listener.

## Considered Options

- **One launch is one Flight** (the relay closes when the listener stops). Rejected. It is simpler and every Flight starts clean, but a listener that crashes or has its window closed takes the relay with it, and recovering then means restarting X-Plane or re-enabling the plugin in Plugin Admin.
- **Relay outlives the listener (chosen).** A restarted listener costs nothing in X-Plane. The relay is closed by hand, for example at the end of the day.

## Consequences

- A relay that is already running keeps the addresses it started with. The launcher does not ask for an iMotions address in that case; it says so, and changing the address means closing the relay.
- Restarting a listener mid-flight must keep the same `flight_id`, or one **Flight** splits into two. The launcher offers to resume when the last Flight did not end with `q`, and `live_groups` then continues the **Brief** from the last **Instruction** given.
- A crash of the relay itself still forces a plugin restart until #256 and #255 are fixed. Once they are, this decision still stands, since it avoids reconnecting at all.
