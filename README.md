# Overview
- `xplane_plugin`: X-Plane's data extraction plugin
- `relay`: External GUI program that takes the data extracted by `xplane_plugin` and sends it to iMotions over TCP
- `baton`: C++ library for communicating with `relay`. Provides an object that handles inter-process communication (IPC) and abstracts over it for ease-of-use. Currently located in `xplane_plugin/subprojects`

# Details
## High-Level View
The data flow is as follows:
- `xplane_plugin` extracts the data
- `xplane_plugin` uses the `baton` library to send the data to the `relay` program
- `relay` sends the data to iMotions

`relay` and `baton` are developed by this team and are not external programs/libraries.

`xplane_plugin` and `relay` are top level subprojects in the repo. It is currently undecided whether `baton` will be top level or not, and so for now will be found under `xplane_plugin/subprojects/`. In the event that we begin work on Prepar3D during this iteration, we can decide if `baton` is generic enough to use for both simulators and move it to the top level, or have a `baton` version for each simulator plugin as an internal dependency.

## Mid-Level View
### Why `baton`?
At the time of writing, the current version of `iceoryx2` does not support cross-language communication. Only C++-to-C++ and Rust-to-Rust communication is possible, not C++-to-Rust. `xplane_plugin` is written in C++ and `relay` is written in Rust. Thus, `iceoryx2` cannot be used to facilitate communication between `xplane_plugin` and `relay`.

However, this limitation only extends to what language the communication is compiled from. Rust-to-Rust communication where one end is compiled to a C library is valid. This is where `baton` comes in. `baton` abstracts over the finer details of `iceoryx2` communication to `relay` and is compiled to C++ despite being a Rust library. This enables the plugin (again, written in C++) to communicate with `relay` (again, written in Rust) when this otherwise would not be possible.

A secondary benefit to using Rust over C++ for `baton` is that we can leverage Rust's superior concurrency and safety guarantees. By managing the threading and communication in the Rust library and providing the plugin only a very limited interface by which it can pass in values to be sent to `relay`, we make a worthwhile exchange. We trade up-front complexity for vastly reduced need for debugging further down the line, as our code is more likely to be sound. This is an important consideration, as the project will be passed on to new students for next iteration. It is a much better use of developer time to work on developing features instead of being mired in deeply complex concurrency concerns similar to the ones that appeared during the first iteration of this project when we began.

## Low-Level View
Work in progress.

### `xplane_plugin`

### `relay`

#### `iced`

#### `iceoryx2`

#### iMotions

### `baton`

#### `iceoryx2`

## Supported versions (CI + Local)

- Rust: stable (toolchain is pinned by `.rust-toolchain.toml`; CI uses the Rust stable channel)
- Python: 3.x (latest 3 series)
- Meson: >= 1.x
- Ninja: latest
- mingw-w64: required for cross-compilation to Windows (target `x86_64-pc-windows-gnu`)
- GoogleTest: for C++ tests

## Local check (recommended)
1. Install rustup and ensure stable is active:
   - `rustup update`
   - `rustup default stable`
2. Install the Windows target:
   - `rustup target add x86_64-pc-windows-gnu`
3. Install meson & ninja:
   - `python -m pip install --user meson ninja`
4. Setup and build:
   - `meson setup --cross-file xplane_plugin/lin-to-win.ini build`
   - `meson compile -C build`
5. Run tests:
   - `meson test -C build`
   - `cd relay && cargo test`
   - `cd xplane_plugin/subprojects/baton && cargo test`
Note: `.rust-toolchain.toml` will cause `rustup` (and CI) to prefer the stable toolchain and auto-install the components/targets listed, keeping local and CI toolchains aligned
