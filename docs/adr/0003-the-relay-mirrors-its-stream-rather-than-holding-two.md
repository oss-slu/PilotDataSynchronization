# The relay mirrors its iMotions stream rather than holding two equal connections

A flight has to reach iMotions and the Python side at the same time (#245). The relay keeps one primary TCP connection, the one `Connect TCP` opens, and an optional **Mirror** that receives a copy of every packet. Both are written from one packet list, so they cannot drift apart. The mirror's socket is owned by its own writer thread, and the TCP thread reaches it only through a bounded queue.

## Considered Options

- **Mirror inside the existing TCP thread.** Simplest, and how this was first written. Rejected after review: the thread wrote each packet to iMotions and then to the mirror, so a mirror write that blocked delayed the next iMotions write, and a connect attempt stalled the stream for the length of its timeout. The recording is the thing that cannot be redone, so it must not wait on a copy.
- **Mirror on its own writer thread, fed by a bounded queue (chosen).** The TCP thread's only mirror cost is pushing into a queue that either accepts the packet or refuses it. Connecting and writing both happen off that thread.
- **Two equal connections, either optional.** Cleaner in principle and would let the Python side run with no iMotions at all. Rejected because it doubles the connection state machine (`tcp_connect`, `tcp_disconnect`, `reap_tcp_thread`, the status line and the saved-address list all assume one stream), and the case it buys is already covered: with no iMotions, point `Connect TCP` straight at the Python listener.
- **Let the Python side read the CSV the logger already writes.** Rejected. Live grouping needs samples as they arrive, and the logger only has them because it is the thing the relay is connected to.

## Consequences

What is guaranteed: the TCP thread never waits on the mirror. Connecting, writing and failing all happen on the writer thread, and the queue is pushed with a non-blocking try-send. What is not guaranteed is delivery of every copy. When the queue is full the packet is dropped, which is the right trade for a consumer doing statistics over a stream, and the wrong one for anything that needs a complete record.

A mirror falling behind is reported once when it starts dropping and once when it catches up, not per packet, because the event log is capped at 200 entries and 160 packets a second would empty it.

The Python side has to read its socket continuously, on a thread of its own if it also does slow work. A clustering refit that stops reading for long enough will first lose copies and then, once the write timeout expires, lose the mirror, which does not reconnect by itself.

Nothing reconnects the mirror automatically. If Python restarts, press `Connect mirror` again.

`Send Packet` reaches both destinations, so it checks both links at once, and it writes a row of zeros to the logger as well as to iMotions.

If the Python side later needs to run without iMotions in the same session, revisit this and promote the mirror to an equal connection.
