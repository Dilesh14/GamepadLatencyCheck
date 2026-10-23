# Gamepad Latency Tester

A native desktop prototype for Windows, macOS, and Linux. It detects connected controllers, shows button and axis changes, and summarizes intervals between input events as received by the application.

## Get the Code

Install Git, then clone the repository and enter its directory:

```sh
git clone https://github.com/Dilesh14/GamepadLatencyCheck.git
cd GamepadLatencyCheck
```

## Run on macOS

- Install the current stable Rust toolchain from [rustup.rs](https://rustup.rs/).
- Install Apple's Command Line Tools if they are not already installed:

  ```sh
  xcode-select --install
  ```

From the repository root, build and launch the app:

```sh
cargo run
```

## Run on Windows

- Install the current stable Rust toolchain from [rustup.rs](https://rustup.rs/) and choose the default MSVC toolchain.
- Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) with the **Desktop development with C++** workload and a Windows SDK.
- Open a new PowerShell window and change to the repository root.

Run the app:

```powershell
cargo run
```

Windows Game Input may require the app window to be focused while reading controller events.

## Run on Linux

- Install the current stable Rust toolchain from [rustup.rs](https://rustup.rs/).
- Install a C compiler, `pkg-config`, and the `libudev` development files. For Debian or Ubuntu:

  ```sh
  sudo apt update
  sudo apt install build-essential pkg-config libudev-dev
  ```

  Other distributions use different package names; install the equivalent compiler, pkg-config, and libudev development packages.

From the repository root, build and launch the app:

```sh
cargo run
```

If no controller appears, check that your user has permission to read gamepad devices under `/dev/input`; the required group or udev setup depends on the distribution.

## What The Timing Means

The displayed interval is the time between input events as the app processes them, measured with a monotonic clock. It is not a measurement of controller polling rate or physical button-to-screen latency. Event queueing, operating-system scheduling, focus, connection type, and controller mappings can all affect what the app observes. Physical end-to-end measurements require a separate, validated hardware-assisted setup.
