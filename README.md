[![Super-Linter](https://github.com/oss-slu/PilotDataSynchronization/actions/workflows/code-linting.yml/badge.svg)](https://github.com/marketplace/actions/super-linter)


# Pilot Data Synchronization

Our project is still in progress, and the current phase focuses on successfully communicating with the research platform! We are also in the process of restructuring our project build, our CI/CD pipeline, our unit tests, and our big-picture architecture. Stay tuned for our improvements! 

## Project Overview

This project is designed to extract key data from the X-Plane flight simulator, including Altitude, Airspeed, Vertical Airspeed, Heading attributes, and transmit it to the iMotions research platform via a TCP connection. The extracted data will be formatted according to iMotions’ API requirements, enabling users to sync the flight data with other biometric sensors in real-time to analyze pilot performance!

<!--
# Getting Started
To perform initial project setup, run the `init.ps1` PowerShell script found in the `utils` directory. This will download a copy of XPLSDK410 and extract it into the `lib` folder. This is necessary for successfully building the plugin.
-->

## Build System
This project uses the Meson build system to build our project! More information can be found on the [Meson homepage](https://mesonbuild.com/).

## Prerequisites

To run the code, ensure you have the following packages installed using your preferred package manager:
- `mingw-w64` : C++ compiler
- `meson` : build system
- `rust` : programming language (pending) 
- Winsock2 Library: Required for socket programming on Windows.


## Getting Started : Step-by-Step Build Instructions
1. Clone the PilotDataSync repo from the github onto your local device
2. Make sure to download and install the following dependencies from your preferred package manager (this step will be updated with package-managemer-specific instructions soon!):
    - `mingw-w64`
    - `meson`
    - `rust`
3. Run the script `./utils/init.sh` (mac/linux) or `./utils/init.ps1` (windows) from the project root to download the XPlane SDK into the generated `./lib` folder 
4. Next, run the following scripts in the project root:
    - `meson setup --cross-file win.ini build` : which initializes the Meson build system
    - `meson compile -C build` : which compiles the meson build system and places the resultant file into the `./build` folder.
And there you go: project built! Currently, the build system in active development and change and we will be updating this README as we go with accurate build instructions!



## Project Layout
- Place source code in the `src/` directory.
- Helpful utilities can be found in the `utils/` directory.
- Tests and documentation go into `tests/` and `docs/` respectively.
- The `.github` folder contains our projects' CI/CD pipeline files and any GitHub templates that we use.
- The XPlane SDK lives in the `lib/` directory, both of which should be automatically generated when you run the initialization scripts. Do not commit and push the SDK
- Plugins, binaries, and artifacts go into the `bin/` directory. Nothing from this directory should ever be pushed to the repo.

## Styling
<!---
 C++ code is formatted using the VSCode C/C++ Extension's format action. The rules are expanded on in `.clang-format`. Submitted code must be formatted accordingly. Invoke it in VSCode by using the command palette -> `Format document with...` -> `C/C++`, which will automatically used the provided formatting rules.
-->

The code formatting requirements have recently changed due to our implementation of a CI/CD code linter and auto-formatter! Stay tuned!

## Contributing

To get started contributing to the project, see the [contributing guide](CONTRIBUTING.md).
This document also includes guidelines for reporting bugs and proposing new features.
