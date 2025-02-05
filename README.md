[![Super-Linter](https://github.com/oss-slu/PilotDataSynchronization/actions/workflows/code-linting.yml/badge.svg)](https://github.com/marketplace/actions/super-linter)


# pilot training data synchronization

Our project is still in progress, and the current phase focuses on data extraction and communication protocols. As the project evolves, additional features and optimizations will be implemented.

## Project Overview

This project is designed to extract key data from the X-Plane flight simulator, including Altitude, Airspeed, Vertical Airspeed, Heading attributes, and transmit it to the iMotions platform via a TCP client connection. The extracted data will be formatted according to iMotions’ API requirements, enabling real-time data synchronization for advanced analysis of pilot performance.

# Getting Started
To perform initial project setup, run the `init.ps1` PowerShell script found in the `utils` directory. This will download a copy of XPLSDK410 and extract it into the `lib` folder. This is necessary for successfully building the plugin.

## Project Layout
- Place source code in the `src/` directory.
- Helpful utilities can be found in the `utils/` directory.
- Place the SDK, as well as any other necessary libraries, in the `lib/` directory. The `utils/init.ps1` script will automatically download, extract, and place the SDK into `lib` for you. Do not commit and push the SDK.
- Tests and documentation go into `tests/` and `docs/` respectively.
- Plugins, binaries, and artifacts go into the `bin/` directory. Nothing from this directory should ever be pushed to the repo.

# Prerequisites

To run the code, ensure you have the following installed:

C++ Compiler: The project is written in C++. [Download and install the 2022 Visual Studio Build Tools to obtain the necessary compiler.](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
Winsock2 Library: Required for socket programming on Windows.
X-Plane Plugin SDK: For accessing flight data from X-Plane.
iMotions API Documentation: To correctly format and transmit data to the platform.

# Styling
C++ code is formatted using the VSCode C/C++ Extension's format action. The rules are expanded on in `.clang-format`. Submitted code must be formatted accordingly. Invoke it in VSCode by using the command palette -> `Format document with...` -> `C/C++`, which will automatically used the provided formatting rules.

## Contributing

To get started contributing to the project, see the [contributing guide](CONTRIBUTING.md).
This document also includes guidelines for reporting bugs and proposing new features.
