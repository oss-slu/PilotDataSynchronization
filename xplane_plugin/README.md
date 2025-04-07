[![Super-Linter](https://github.com/oss-slu/PilotDataSynchronization/actions/workflows/code-linting.yml/badge.svg)](https://github.com/marketplace/actions/super-linter)

# Pilot Training Data Synchronization: XPlane Plugin
Our project is still in progress, and the current phase focuses on data extraction and communication protocols. As the project evolves, additional features and optimizations will be implemented.

## Project Overview
This project is designed to extract key data from the X-Plane flight simulator, including Altitude, Airspeed, Vertical Airspeed, Heading attributes, and transmit it to the iMotions platform via a TCP client connection. The extracted data will be formatted according to iMotions’ API requirements, enabling real-time data synchronization for advanced analysis of pilot performance.

## Prerequisites
To run the code, ensure you have the following packages installed using your preferred package manager:
- `mingw-w64` : C++ compiler toolchain
- `meson` : build system
- `rust` : programming language
- Winsock2 Library: Required for socket programming on Windows. (deprecated, being removed)
- GoogleTest: Testing framework for unit tests.

## Getting Started : Step-by-Step Build Instructions
1. Clone the PilotDataSync repo from the github onto your local device
2. Make sure to download and install the following dependencies from your preferred package manager (this step will be updated with package-managemer-specific instructions soon!):
    - `mingw-w64`
    - `meson`
    - `rust`
    - `gtest`
3. Run `rustup target add x86_64-pc-windows-gnu` to ensure you have the correct build target installed for the Rust toolchain
4. Next, run the following scripts in the project root:
    - `meson setup --cross-file [CROSSFILE] build` : which initializes the Meson build system
      - On Linux/WSL2, replace `[CROSSFILE]` with `lin-to-win.ini`
      - On MacOS, replace `[CROSSFILE]` with `mac-to-win.ini`
      - On Windows, omit the `--cross-file` flag entirely
    - `meson compile -C build` : which compiles the meson build system and places the resultant file into the `./build` folder.
      - `meson compile -C build tests` builds just the test executable
      - `meson compile -C build baton` builds `baton` and its bindings
5. You can find the compiled `PilotDataSync.xpl` binary in the `build/` directory. It must be placed in X-Plane's Resources directory to function.
And there you go: project built! Currently, the build system in active development and change and we will be updating this README as we go with accurate build instructions!

## Project Layout
- Place source code in the `src/` directory.
- Helpful utilities can be found in the `utils/` directory.
- Tests and documentation go into `tests/` and `docs/` respectively.
- The `.github` folder contains our projects' CI/CD pipeline files and any GitHub templates that we use.
- Plugins, binaries, and artifacts go into the `bin/` directory. Nothing from this directory should ever be pushed to the repo.

### Baton
Our X-Plane plugin uses a library written for this project that we call Baton, located in `subprojects/baton`. This library handles IPC communication between the plugin and Relay. It is written in Rust and compiles to C++-compatible code. Check its `README.md` for more, as it is essential for this project.

## Styling
<!---
 C++ code is formatted using the VSCode C/C++ Extension's format action. The rules are expanded on in `.clang-format`. Submitted code must be formatted accordingly. Invoke it in VSCode by using the command palette -> `Format document with...` -> `C/C++`, which will automatically used the provided formatting rules.
-->
The code formatting requirements have recently changed due to our implementation of a CI/CD code linter and auto-formatter! Stay tuned!

## Contributing
To get started contributing to the project, see the [contributing guide](CONTRIBUTING.md).
This document also includes guidelines for reporting bugs and proposing new features.
