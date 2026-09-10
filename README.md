# Overview
- `xplane_plugin`: X-Plane's data extraction plugin
- `relay`: External GUI program that takes the data extracted by `xplane_plugin` and sends it to iMotions over TCP
- `baton`: C++ library for communicating with `relay`. Provides an object that handles inter-process communication (IPC) and abstracts over it for ease-of-use. Currently located in `xplane_plugin/subprojects`

Additional Components:
- `inference`: Python-based module for logging, labeling, and training a machine learning model using the collected flight data

# Setup

Start the components in this order: iMotions (or the mock server) → relay → X-Plane. The relay is a TCP client, and any flight data that arrives before its TCP connection is up is discarded.

## Prerequisites

| Tool | Version | Needed for | Notes |
|---|---|---|---|
| [rustup](https://rustup.rs) / Rust | stable | `relay`, `baton` | On Windows also run `rustup target add x86_64-pc-windows-gnu` |
| Meson | >= 1.7.0 | `xplane_plugin` | `python -m pip install "meson>=1.7.0"` |
| Ninja | latest | `xplane_plugin` | `python -m pip install ninja` |
| Python 3 | 3.x | `inference`, and the plugin build | Must be callable as `python3`. See the note below |
| mingw-w64 | any version supporting C++20 | `xplane_plugin` | Needed for native Windows builds too, not only for cross-compiling |

The X-Plane SDK and GoogleTest are not manual installs. Meson downloads both automatically during `meson setup`, using the `.wrap` files in `xplane_plugin/subprojects/`. A failure at that step is a network problem, not a missing folder.

> `python3` must be on your PATH. The baton build shells out to `python3 script.py` (see `xplane_plugin/subprojects/baton/meson.build`). On Windows the command is frequently only `python` or `py`, and the failure surfaces much later as `FileNotFoundError` from `shutil.copyfile`. Check with `python3 --version` before you build.

## Build everything

```bash
# 1. The X-Plane plugin (this also builds baton, one of its subprojects)
cd xplane_plugin
meson setup build                                # Windows - native build, no cross-file
# meson setup --cross-file lin-to-win.ini build  # Linux / WSL2, cross-compiling to Windows
# meson setup --cross-file mac-to-win.ini build  # macOS, cross-compiling to Windows
meson compile -C build
meson test -C build

# 2. The relay
cd ../relay
cargo build

# 3. baton's own tests
cd ../xplane_plugin/subprojects/baton
cargo test
```

Every `meson compile` rebuilds baton from scratch, and baton is compiled with fat LTO, so expect roughly 30 seconds even when nothing changed.

## 1. X-Plane plugin

The build produces `xplane_plugin/build/PilotDataSync.xpl`.

Install it into your X-Plane installation using the SDK 3.0 packaging schema, `<plugin name>/<ABI>/<plugin name>.xpl`, beneath `Resources/plugins`:

```
X-Plane 12/Resources/plugins/PilotDataSync/win_x64/PilotDataSync.xpl
```

Substitute `mac_x64` or `lin_x64` for `win_x64` on those platforms. This layout is documented in `SDK/README.txt` inside the SDK that Meson downloads to `xplane_plugin/subprojects/x-plane-sdk-4.0.1/`.

> Not yet confirmed against a live installation. The path above comes from the SDK's own documentation, but no one has verified it by loading the plugin into a running copy of X-Plane. If you have one, please confirm it and delete this note.

There is nothing to configure in the plugin. It has no IP address, no port, no config file and no environment variable. It communicates only with the relay, over a local IPC socket named `baton.sock` that is hardcoded. The iMotions address is entered in the relay's window, not here.

Once X-Plane loads the plugin, it connects on its own:

- A floating window titled "Positional Flight Data" appears, showing the live values.
- baton starts automatically and begins retrying its connection to the relay.
- There is no menu item, no hotkey, and no button to press to start sending.

> On Windows and Linux, data flows only while the plugin window is being drawn. The 20 Hz send lives inside that window's draw callback, so closing or hiding "Positional Flight Data" stops the stream completely. (macOS uses a flight-loop callback instead and is unaffected.)

baton retries the relay connection about 20 times across roughly 90 seconds, then gives up permanently. If the relay was not running during that window, disable and re-enable the plugin in X-Plane. Starting the relay afterwards will not recover the connection on its own.

## 2. Relay

Run it from a terminal:

```bash
cd relay
cargo run
```

The relay has no log panel in its window, and every diagnostic message goes to the terminal instead.

Then, in order:

1. Start iMotions, or the mock server, first, so that something is listening.
2. Launch the relay. The IPC listener that the plugin connects to starts automatically. You do not need to press `Connect IPC`. That button exists only to reconnect after you have pressed `Disconnect IPC`.
3. Type the iMotions address into the text box, for example `127.0.0.1:9999`.
   - There is no default value. The greyed-out `127.0.0.1:9999` is placeholder text, not a setting.
   - A literal IP address and port are required. `localhost:9999` is rejected.
   - Addresses that connect successfully are remembered in the `Saved IPs` dropdown (stored on Windows at `%APPDATA%\PilotDataSynchronization\relay_ips.json`).
4. Press `Connect TCP`.
5. Press `Check TCP Connection Status` to refresh the status line. It does not update by itself. This button is the only thing that changes it.
6. Start X-Plane. Flight data now flows automatically.

> The `Send Packet` button does not send anything. It only records a timestamp, which is not displayed anywhere. Ignore it, the data stream is automatic.

Two more behaviors that catch people out:

- The eight dataref toggles do not affect what is sent over TCP. All eight values are always transmitted. The toggles control only which `<Sample>` entries appear in the generated `iMotions.xml`.
- Losing X-Plane also tears down the TCP connection. After restarting the simulator, press `Connect TCP` again.

## 3. iMotions

With the relay running, generate the event-source definition that iMotions needs:

1. Press `Open XML Download Menu`.
2. Leave the toggles you want enabled. All eight are on by default.
3. Press `Generate XML File`.

The file is written to your Downloads folder as `iMotions.xml` (`%USERPROFILE%\Downloads\iMotions.xml` on Windows). It contains one `<Sample>` per enabled toggle, beneath:

```xml
<EventSource Version="1" Id="PilotDataSync" Name="Positional Flight Data">
```

The `Id="PilotDataSync"` is what ties the definition to the live stream. Every packet the relay sends carries the same source name:

```
E;1;PilotDataSync;;;;;AltitudeSync;1250.5;1250.5
```

Packets are semicolon-delimited and CRLF-terminated. The value appears twice because each sample carries both a `FlightModel*` and a `Pilot*` field.

> Needs verification. How `iMotions.xml` is imported into iMotions, and which port iMotions actually listens on, are not documented anywhere in this repository. `127.0.0.1:9999` appears only as placeholder text in the relay's UI. Nothing confirms it is iMotions' real port. If you have iMotions access, please confirm both and update this section.

### No iMotions? Use the mock server

`src/server/` is a minimal TCP listener you can point the relay at to confirm that it connects and transmits:

```bash
cd src/server
cargo run          # listening on port 7878
```

Enter `127.0.0.1:7878` in the relay and press `Connect TCP`.

> The mock server reads a single buffer per connection and does not loop, so it prints only the first packet it receives, not a continuous stream. It is enough to prove that the relay connects and sends, and no more.

## Verifying it works

In the relay's terminal, you should see roughly this sequence:

```
[RELAY] listening on socket: "baton.sock"
✓ Successfully created named pipe listener
[...] TCP - Successfully connected to iMotions server.
[...] TX - packet len=50 text="E;1;PilotDataSync;;;;;AltitudeSync;1250.5;1250.5\r\n"
```

In the relay's window:

- `:) Baton Connected!`
- `TCP Connection Status: true`, after pressing `Check TCP Connection Status`.

> Expect about five seconds of zeros when the plugin first connects. baton performs a greeting handshake and then sends 15 dummy `0` values as a frequency test before real flight data begins.

## Troubleshooting

| Symptom | Cause | What to do |
|---|---|---|
| `Connect TCP` appears to do nothing and the status stays `false` | The connection failed. No error is displayed, and pressing `Connect TCP` again fails silently because the previous thread still exists. | Press `Disconnect TCP` first, then `Connect TCP`. This is the most common first-time snag. |
| `:( No Baton Connection` | The plugin is not loaded, or its window is closed | Check `baton_debug.log` in your temp folder (`%TEMP%` on Windows). Every connection attempt is logged there |
| Data stops part-way through a flight | The "Positional Flight Data" window was closed or hidden (Windows/Linux only) | Reopen it |
| The relay runs but never picks up the plugin | The socket is already in use. A second relay instance, or a stale `baton.sock`. The relay logs this once, then gives up silently. | `Disconnect IPC`, then `Connect IPC`. Or restart the relay |
| The plugin never connects, even after the relay is started | baton gave up after roughly 90 seconds of retries and does not re-arm | Disable and re-enable the plugin in X-Plane |
| `meson setup` fails while downloading subprojects | The X-Plane SDK and GoogleTest are fetched over the network | Check connectivity or proxy settings, then re-run `meson setup build` |
| The baton build fails with `FileNotFoundError` from `shutil.copyfile` | `python3` is not on PATH, or the Rust target is missing | Confirm `python3 --version`, and run `rustup target add x86_64-pc-windows-gnu` |

# Details
## High-Level View
The data flow is as follows:
- `xplane_plugin` extracts the data
- `xplane_plugin` uses the `baton` library to send the data to the `relay` program
- `relay` sends the data to iMotions
- *(optional)* `inference` can log the same TCP data stream for dataset creation and model training

`relay` and `baton` are developed by this team and are not external programs/libraries.

`xplane_plugin` and `relay` are top level subprojects in the repo. It is currently undecided whether `baton` will be top level or not, and so for now will be found under `xplane_plugin/subprojects/`. In the event that we begin work on Prepar3D during this iteration, we can decide if `baton` is generic enough to use for both simulators and move it to the top level, or have a `baton` version for each simulator plugin as an internal dependency.

## Data Being Sent
The system currently sends 8 flight parameters:
- altitude  
- heading  
- vertical speed  
- airspeed / velocity  
- roll  
- pitch  
- yaw  
- g-force  

These are mapped in the relay to iMotions events such as `AltitudeSync`, `RollSync`, `YawSync`, etc.

## Mid-Level View
### Why `baton`?
At the time of writing, the current version of `iceoryx2` does not support cross-language communication. Only C++-to-C++ and Rust-to-Rust communication is possible, not C++-to-Rust. `xplane_plugin` is written in C++ and `relay` is written in Rust. Thus, `iceoryx2` cannot be used to facilitate communication between `xplane_plugin` and `relay`.

However, this limitation only extends to what language the communication is compiled from. Rust-to-Rust communication where one end is compiled to a C library is valid. This is where `baton` comes in. `baton` abstracts over the finer details of `iceoryx2` communication to `relay` and is compiled to C++ despite being a Rust library. This enables the plugin (again, written in C++) to communicate with `relay` (again, written in Rust) when this otherwise would not be possible.

A secondary benefit to using Rust over C++ for `baton` is that we can leverage Rust's superior concurrency and safety guarantees. By managing the threading and communication in the Rust library and providing the plugin only a very limited interface by which it can pass in values to be sent to `relay`, we make a worthwhile exchange. We trade up-front complexity for vastly reduced need for debugging further down the line, as our code is more likely to be sound. This is an important consideration, as the project will be passed on to new students for next iteration. It is a much better use of developer time to work on developing features instead of being mired in deeply complex concurrency concerns similar to the ones that appeared during the first iteration of this project when we began.

#Additional Note:
Recent updates include non-blocking IPC handling in `baton`, improved shutdown handling (killswitch flags), and more stable relay connection management to prevent repeated connection bugs.

## Visual Overview 
### Sample Metric Graphs
Here is a link to the same metric graphs, the reason for this is so that youre able to see how the data is supposed to look, just in case you want to add to this project but you dont have access to Imotions. 
https://docs.google.com/document/d/1KRa0qkovk8kHhXT9Z3p66wXcQFLISTJb8F0M-sINLZ8/edit?usp=sharing 

### System Data Flow Diagram
For total understanding of how the data is actually being sent and where it works here is a flow diagram that shows how the data is being sent. 
https://docs.google.com/document/d/1KRa0qkovk8kHhXT9Z3p66wXcQFLISTJb8F0M-sINLZ8/edit?usp=sharing 

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

- Rust: stable (CI uses the Rust stable channel)
- Python: 3.x (latest 3 series)
- Meson: >= 1.7.0 (required by `xplane_plugin/meson.build`)
- Ninja: latest
- mingw-w64: required to build the plugin. Both natively on Windows and when cross-compiling to Windows from Linux/macOS (target `x86_64-pc-windows-gnu`)
- GoogleTest: for C++ tests (downloaded automatically by Meson)

## Local check (recommended)

A quick way to confirm your toolchain is healthy. For the full walkthrough, see [Setup](#setup) above.

1. Install rustup and ensure stable is active:
   - `rustup update`
   - `rustup default stable`
2. Install the Windows target:
   - `rustup target add x86_64-pc-windows-gnu`
3. Install meson & ninja:
   - `python -m pip install --user "meson>=1.7.0" ninja`
4. Setup and build (run from `xplane_plugin/`):
   - `cd xplane_plugin`
   - `meson setup build` on Windows. From Linux/WSL2 use `meson setup --cross-file lin-to-win.ini build`. For macOS use `meson setup --cross-file mac-to-win.ini build`
   - `meson compile -C build`
5. Run tests:
   - `meson test -C build`
   - `cd ../relay && cargo test`
   - `cd ../xplane_plugin/subprojects/baton && cargo test`
#Additional Validation Notes:
- Data logging has been tested with hundreds of samples with no missing values  
- Labeling pipeline successfully processed over 16k rows, though some flight phase labels are still missing  
- Final validation still requires a full real-time flight session with X-Plane + iMotions connected  

Note: the file `toml .rust-toolchain.toml` at the repository root is not currently in effect. `rustup` only reads `rust-toolchain.toml` (or `rust-toolchain`), so the leading `toml ` in the filename means the toolchain is not pinned locally or in CI. Both use the default stable toolchain. Renaming the file would activate the `channel = "1.80.0"` pin inside it, which has not been tested against the current dependencies.
