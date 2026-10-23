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

Input is read by a dedicated polling thread (roughly every 1 ms), so intervals reflect when this app received consecutive events rather than once-per-frame snapshots. Idle gaps between bursts of activity are included in the statistics and the plot.

## Diagnostics Screen

Select a controller in the sidebar; the first detected controller is selected automatically. The diagnostics screen shows:

- Live button and axis state. Gilrs uses positional button names, with common Xbox/PlayStation equivalents shown in brackets.
- Sample count, minimum, mean, median, p95, and maximum for the retained interval window.
- A plot of the most recent intervals and the selected controller's device details.
- **Reset samples** to clear the retained window, and **Export CSV** for the current window.

If a controller disconnects, its last state and retained samples remain visible until you select another controller or it reconnects.

## Visual Reaction Test

Choose **Reaction test**, select a connected controller in the sidebar, and start a session. The app shows a randomly selected circle or square after a 1.5-4 second wait. Press any button on that controller when the cue appears. A press before the cue is a false start and restarts the trial. The default session contains 20 successful trials; the screen shows recent trial times and session mean, p95, and best time. Disconnecting the test controller pauses the session; reconnect and select a controller to resume. Leaving the Reaction test tab also pauses an active session so the cue cannot pass unseen; return to the tab and resume to restart the current trial.

This is **app-observed visual reaction time**, measured from the frame when the app submits the cue for drawing to when it handles the controller event. It includes human response, event queueing, and frame/display scheduling; it does not measure controller hardware latency or the exact time pixels appear. Reaction results remain in memory for the current app session and are not included in the diagnostics CSV export.

## Exporting a Session

Choose **Export CSV** to save session metadata and the selected controller's retained interval samples. The app keeps a rolling window of up to 512 intervals, so the CSV contains that recent window rather than an unlimited session history. The exported samples describe app-observed event processing intervals and are not physical latency measurements.

Files are written to your `Downloads` folder (falling back to your home folder) with a timestamped name such as `gamepad-intervals-20261023-141502.csv`. Metadata is stored as leading `#` comment lines, followed by a header row and one row per interval sample.
