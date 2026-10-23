# Gamepad Latency Tester

## Goal

Build a native desktop app for Windows, macOS, and Linux that helps users inspect controller inputs and understand their timing behavior.

## Proposed Stack

- **Language:** Rust.
- **Controller input:** `gilrs` as the first library to evaluate for cross-platform gamepad enumeration and input events.
- **Desktop UI:** `egui` with `eframe` as the first lightweight native UI option to evaluate.
- Confirm library support, packaging requirements, and input behavior on all three operating systems with a prototype before committing to the stack.

## Measurement Scope

The software can report timings it observes, such as event arrival intervals, sampling cadence, and time from input observation to app-side display or logging. These values are not physical controller-to-screen latency. The roadmap also includes a hardware-assisted path for end-to-end physical measurement, using external instrumentation or a validated loopback method. Keep the two measurement modes distinct in the UI and reports; select the hardware approach only after a feasibility spike.

## Initial Milestones

1. **Platform and API spike:** Verify controller discovery, button/axis updates, timestamps, and build requirements on Windows, macOS, and Linux.
2. **Input prototype:** Show the connected controller and live button/axis state; record monotonic timestamps for observed changes.
3. **MVP measurements:** Summarize event intervals and app-side response timing, show sample count and basic distribution statistics, and explain the limitations in the UI.
4. **Usability and export:** Support reconnecting devices, clear empty/error states, and optional CSV export of raw samples and session metadata.
5. **Hardware-assisted measurement spike:** Compare feasible sensor or loopback approaches for measuring physical input-to-display latency; document required hardware, calibration, platform coverage, and uncertainty before choosing an implementation.
6. **Release readiness:** Build and smoke-test each target OS, document supported controllers and known platform differences, and make physical-latency claims only for validated hardware setups.

## MVP Acceptance Criteria

- Runs as a desktop app on Windows, macOS, and Linux.
- Detects a connected gamepad and visibly reports button and axis changes.
- Uses a monotonic clock for recorded intervals and identifies each metric in plain language.
- Makes it clear that software-observed timing is not end-to-end physical latency.
- Handles no device, disconnect/reconnect, and unsupported-device states without crashing.

## Risks and Open Decisions

- `gilrs` and UI backend capabilities, permissions, and packaging differ by OS; verify them in the platform spike.
- Controller firmware, wired versus wireless transport, OS scheduling, focus, and background load can affect observations.
- Decide after the prototype whether charts, multi-controller sessions, and CSV export belong in the first release.
- Choose between high-speed-camera, electrical loopback, or another validated external measurement method after the hardware-assisted spike.
