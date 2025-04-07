TODO

`baton` is an interface between `xplane_plugin`, which extacts flight data, and `relay`, which sends it to iMotions over TCP.

The purpose of creating this interface is to enable compatibility with Rust and to abstract IPC thread management away from C++, as it is easier to reason about in Rust.

`baton` uses the `cxx` Rust crate to generate the C++ bindings used by `xplane_plugin`. Again, the goal is to minimize the visibility of threads and IPC from the C++ side of things. As such, the essential functions exposed by this interface are `start`, `stop`, and `send`, with self-explanatory purpose. All other details are implemented in the Rust code and are opaque to the C++ side. 
