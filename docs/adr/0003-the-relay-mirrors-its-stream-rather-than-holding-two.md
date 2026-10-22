# The relay mirrors its iMotions stream rather than holding two equal connections

A flight has to reach iMotions and the Python side at the same time (#245). The relay keeps one primary TCP connection, the one `Connect TCP` opens, and an optional mirror that receives a copy of every packet. Both are written from one packet list inside the TCP thread, so they get identical bytes. The mirror is best effort: a failed connect or write drops it, reports it in the event log, and leaves the primary stream untouched.

## Considered Options

- **Mirror inside the existing TCP thread (chosen).** One packet list, so the two sinks cannot drift. The primary keeps its existing behaviour exactly, which matters because iMotions is the recording that cannot be redone. The cost is that the mirror depends on the primary: `Connect TCP` has to succeed first, and dropping it drops the mirror too.
- **Two equal connections, either optional.** Cleaner in principle and would let the Python side run with no iMotions at all. Rejected for now because it doubles the connection state machine (`tcp_connect`, `tcp_disconnect`, `reap_tcp_thread`, the status line and the saved-address list all assume one stream), and the case it buys is already covered: with no iMotions, point `Connect TCP` straight at the Python listener.
- **Fan out in a second thread fed by a channel.** Keeps the sinks independent, but the packets would be built twice or shipped between threads, and a thread per sink is more machinery than one `Option<TcpStream>`.
- **Let the Python side read the CSV the logger already writes.** Rejected. Live grouping needs the samples as they arrive, and the logger only has them because it is the thing the relay is connected to.

## Consequences

The mirror can never slow the recording. Its connect and write timeouts are 500ms and 100ms, short on purpose: a Python process that is wedged or gone is dropped rather than allowed to stall a 20 Hz feed. A connect attempt still blocks the TCP thread for up to those 500ms, so packets queue during it and are sent after. That is bounded, happens only on a button press, and loses nothing.

Nothing reconnects the mirror by itself. If Python restarts, press `Connect Python` again. Automatic retry can be added later if sessions turn out to need it.

`Send Packet` reaches both sinks, so it checks both links at once, and it writes a row of zeros to the logger as well as to iMotions.

If the Python side later needs to run without iMotions in the same session, revisit this and promote the mirror to an equal connection.
