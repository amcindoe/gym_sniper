# gym_sniper

A command-line tool to automatically book gym classes on Perfect Gym portals at the exact moment the booking window opens.

## Problem

Popular gym classes fill up within seconds of the booking window opening. The booking window is typically **7 days and 2 hours** before the class starts. This tool automates the booking process so you don't have to be at your computer at the exact right moment.

## Features

- **Login** - Test your credentials
- **List** - View available classes with their booking status
- **Book** - Book a specific class by ID
- **Bookings** - View your booked classes and waitlist positions
- **Snipe** - Wait for booking window and aggressively book a specific class
- **Schedule** - Run continuously and auto-book configured classes when the window opens

## Installation

### Prerequisites

- Rust toolchain (install via [rustup](https://rustup.rs/))

### Build

```bash
cd gym_sniper
cargo build --release
```

The binary will be at `./target/release/gym_sniper`.

## Configuration

Edit `config.toml` with your details:

```toml
[gym]
base_url = "https://labspa.perfectgym.com/clientportal2"
club_id = 2

[credentials]
email = "your-email@example.com"
password = "your-password"

# Classes to auto-book when running in schedule mode
[[targets]]
class_name = "Pilates"
days = ["Monday", "Wednesday"]  # Optional: only these days
time = "10:30"                  # Optional: only this time (HH:MM)

[[targets]]
class_name = "Yoga"
# No days/time filter = book any matching class
```

### Target Options

| Field | Required | Description |
|-------|----------|-------------|
| `class_name` | Yes | Partial match on class name (case-insensitive) |
| `days` | No | List of days: "Monday", "Tuesday", etc. |
| `time` | No | Specific time in HH:MM format |

## Usage

### Test Login

```bash
./target/release/gym_sniper login
```

### List Classes

```bash
# List classes for next 7 days (default)
./target/release/gym_sniper list

# List classes for next 14 days
./target/release/gym_sniper list -d 14
```

Output shows:
- **ID** - Use this to book manually
- **Name** - Class name
- **Time** - Day and time
- **Status** - Booking availability

### Class Statuses

| Status | Meaning |
|--------|---------|
| Bookable | Available to book now |
| Awaitable | Full, can join waitlist |
| Awaiting | You're on the waitlist |
| Booked | You've booked this class |
| Unavailable | Already started or ended |

### Book a Class

```bash
./target/release/gym_sniper book 75738
```

### View Your Bookings

```bash
./target/release/gym_sniper bookings
```

Shows your booked and waitlisted classes with waitlist position:

```
ID       Name                           Time                 Status       Waitlist
----------------------------------------------------------------------------------
75789    Pilates Matwork                Tue 03 Feb 10:30     Awaitable    #8
75813    Vinyasa/Flow Yoga              Wed 04 Feb 08:00     Booked       -
```

### Snipe a Class

For high-demand classes, use snipe mode to book the instant the window opens:

```bash
./target/release/gym_sniper snipe 76014
```

The sniper uses a **polling-based approach** to detect exactly when a class becomes bookable:

1. Display target class and estimated booking window
2. Poll the class status at adaptive intervals:
   - Every 60s when >30 min from estimated window
   - Every 30s when 5-30 min away
   - Every 10s when 1-5 min away
   - Every 2s when <1 min away or past estimated time
3. Automatically refresh login token periodically (safe to run overnight)
4. When status changes to "Bookable", immediately start booking attempts
5. Attempt to book with random delays (200-500ms) to appear human-like
6. Stop after success or ~10 minutes of trying

This approach is more reliable than calculated timing because it detects the actual API status change rather than estimating when the window opens.

Run in background (for overnight waits):
```bash
nohup ./target/release/gym_sniper snipe 76014 > snipe.log 2>&1 &

# Check progress
tail -f snipe.log

# Stop if needed
pkill -f "gym_sniper snipe"
```

### Run Auto-Scheduler

```bash
./target/release/gym_sniper schedule
```

The scheduler:
1. Checks for matching classes every minute
2. Waits until the booking window opens (7 days + 2 hours before class)
3. Books immediately when the window opens
4. Logs success/failure

## Running as a Service

To run the scheduler continuously in the background:

### Using systemd (Linux)

Create `/etc/systemd/system/gym-sniper.service`:

```ini
[Unit]
Description=Gym Class Auto-Booker
After=network.target

[Service]
Type=simple
User=your-username
WorkingDirectory=/path/to/gym_sniper
ExecStart=/path/to/gym_sniper/target/release/gym_sniper schedule
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable gym-sniper
sudo systemctl start gym-sniper

# Check status
sudo systemctl status gym-sniper

# View logs
journalctl -u gym-sniper -f
```

## Debugging

Enable debug logging:

```bash
RUST_LOG=gym_sniper=debug ./target/release/gym_sniper list
```

## Technical Notes

The tool interacts with the Perfect Gym API in a browser-like manner:

- **Browser headers** - Sends User-Agent, Origin, Referer, Accept-Language
- **Random delays** - 200-500ms between requests to appear human-like
- **Session cookies** - Maintains cookies like a real browser session

## Security Note

Your credentials are stored in plain text in `config.toml`. Keep this file secure:

```bash
chmod 600 config.toml
```

## License

MIT
