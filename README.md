# nvwrites

Checks if the NVIDIA driver bug from November 2026 is writing to your SSD nonstop.

GeForce drivers 621.14 and 621.24 make `NVDisplay.Container.exe` keep rewriting a log file at `C:\ProgramData\NVIDIA Corporation\nvtopps\nct\nvlog.nvlgstg`. Users report anywhere from 3 GB to 30 GB an hour. Driver 621.01 doesn't do this.

nvwrites tells you how much that process has written since it started, how fast it's writing right now, and whether that looks like the bug.

## Run it

1. Download `nvwrites.exe` from the Releases page.
2. Double-click it.
3. Click **Yes** when Windows asks for admin.
4. Wait 10 seconds and read the verdict.

It asks for admin because Windows won't let a normal program read the NVIDIA service's counters.

The exe isn't signed, so Windows may show "Windows protected your PC". Click **More info**, then **Run anyway**.

To measure for longer, pass the number of seconds from a terminal:

```powershell
.\nvwrites.exe 30
```

## Build it yourself

Install Rust from [rustup.rs](https://rustup.rs), then:

```powershell
cargo build --release
```

The exe lands in `target\release\nvwrites.exe`.

## Example

```text
nvwrites - NVDisplay.Container.exe disk writes

PC uptime: 0h 3m
  PID   2412: 1.8 MB written since it started
  PID   4384: 3.7 MB written since it started
Total since boot (approx.): 5.5 MB
Average: 1.8 MB/min

Measuring live rate for 10s...
Live rate: 0.0 MB/min (~0.0 MB/day if left on)

Verdict: OK - normal amount of writing
```

## What the verdict means

| Live rate | Verdict |
|---|---|
| Under 2 MB/min | OK |
| 2 to 20 MB/min | ELEVATED. Check again after the PC has been on for an hour. |
| Over 20 MB/min | AFFECTED. Roll back or update the driver. |

The rate climbs the longer the PC stays on. A low number right after boot doesn't mean you're clear.

## How to fix it

Roll back to driver 621.01.

- In the NVIDIA App, go to Drivers, click the menu on the 621.01 row, and install it.
- If that skips the graphics driver, download 621.01 from [nvidia.com/drivers](https://www.nvidia.com/en-us/drivers/), run it, pick **Custom**, and tick **Perform a clean installation**.

Then reboot and run nvwrites again.

## Limits

- Windows only.
- The numbers count from when each process started. If the NVIDIA service restarted after boot, the total covers less than your full uptime.
- It counts what the process asks Windows to write. The SSD itself may write more.
- The thresholds come from user reports. NVIDIA hasn't published numbers.

## Tests

```powershell
cargo test
```

## License

MIT
