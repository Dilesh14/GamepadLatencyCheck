# Gamepad Latency Tester

A native desktop prototype for Windows, macOS, and Linux. It detects connected controllers, shows button and axis changes, and summarizes intervals between input events as received by the application.

## Run

Install a current stable Rust toolchain, then run:

```sh
cargo run
```

On Debian or Ubuntu, `gilrs` may require the `libudev` development package and `pkg-config` (`libudev-dev` and `pkg-config`). Other Linux distributions may package these under different names.

## What The Timing Means

The displayed interval is the time between input events as the app processes them, measured with a monotonic clock. It is not a measurement of controller polling rate or physical button-to-screen latency. Event queueing, operating-system scheduling, focus, connection type, and controller mappings can all affect what the app observes. Physical end-to-end measurements require a separate, validated hardware-assisted setup.