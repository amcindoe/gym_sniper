# gym_sniper

A command-line tool to automatically book gym classes on Perfect Gym portals at the exact moment the booking window opens.

## Problem

Popular gym classes fill up within seconds of the booking window opening. The booking window is typically **7 days and 2 hours** before the class starts. This tool automates the booking process so you don't have to be at your computer at the exact right moment.

## Features

- **Login** - Test your credentials
- **List** - View available classes with their booking status
- **Trainer** - Search classes by trainer name
- **Upcoming** - View classes not yet bookable (window not open)
- **Book** - Book a specific class by ID
- **Bookings** - View your booked classes and waitlist positions
- **Snipe** - Wait for booking window and book immediately when it opens
- **Snipe Add/Remove** - Queue multiple classes to snipe (one per day limit)
- **Snipe List** - View queued snipes and their status
- **Snipe Daemon** - Run continuously and auto-snipe all queued classes
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

### Email Notifications (Optional)

Get notified when a class is booked or when booking fails:

```toml
[email]
smtp_server = "smtp.gmail.com"
smtp_port = 587
username = "your-email@gmail.com"
password = "your-app-password"
from = "Gym Sniper <your-email@gmail.com>"
to = "your-email@gmail.com"
```

**Gmail users:** You need to create an [App Password](https://support.google.com/accounts/answer/185833) (not your regular password).

**Other providers:**
| Provider | SMTP Server | Port |
|----------|-------------|------|
| Gmail | smtp.gmail.com | 587 |
| Outlook | smtp.office365.com | 587 |
| Yahoo | smtp.mail.yahoo.com | 587 |

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
- **Trainer** - Instructor name
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

### Search by Trainer

```bash
# Search for classes by trainer (28 days by default)
./target/release/gym_sniper trainer leona

# Search with custom date range
./target/release/gym_sniper trainer leona -d 14
```

The search is case-insensitive and matches partial names.

### View Upcoming Classes (Not Yet Bookable)

```bash
# Show unbookable classes for next 7 days (default)
./target/release/gym_sniper upcoming

# Show up to 21 days
./target/release/gym_sniper upcoming -d 21
```

Shows classes where the booking window hasn't opened yet, along with when the window opens:

```
ID       Name                           Trainer        Time                 Window Opens
-----------------------------------------------------------------------------------------------
76045    BodyPump                       Sarah          Wed 11 Feb 09:15     Tue 04 Feb 07:15
```

Use this to find class IDs for sniping.

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
ID       Name                           Trainer        Time                 Status       Waitlist
-------------------------------------------------------------------------------------------------
75789    Pilates Matwork                Leonora        Tue 03 Feb 10:30     Awaiting     #8
75813    Vinyasa/Flow Yoga              Sarah          Wed 04 Feb 08:00     Booked       -
```

### Snipe a Class

For high-demand classes, use snipe mode to book the instant the window opens:

```bash
./target/release/gym_sniper snipe 76014
```

The sniper is optimised for precise timing since booking windows open reliably on schedule:

1. Display target class and booking window time
2. Sleep until 1 minute before window opens (no API calls)
3. Refresh login token
4. Sleep until exactly when the booking window opens
5. Start booking attempts immediately
6. Attempt booking every 200ms, max 10 attempts
7. Stop immediately on permanent failures (e.g., daily booking limit reached)
8. If class is full, attempt to join waitlist

This is efficient for overnight sniping and maximises chance of getting a spot.

Run in background (for overnight waits):
```bash
nohup ./target/release/gym_sniper snipe 76014 > snipe.log 2>&1 &

# Check progress
tail -f snipe.log

# Stop if needed
pkill -f "gym_sniper snipe"
```

### Snipe Queue

For managing multiple classes to snipe, use the snipe queue. Only one class per day is allowed (due to gym booking limits).

```bash
# Add a class to the queue
./target/release/gym_sniper snipe-add 76014

# View queued snipes
./target/release/gym_sniper snipes

# Remove a class from the queue
./target/release/gym_sniper snipe-remove 76014
```

The queue is stored in `snipes.json` and enforces one class per day.

### Snipe Daemon

Run the daemon to automatically snipe all queued classes:

```bash
./target/release/gym_sniper snipe-daemon
```

The daemon:
1. Monitors the snipe queue continuously
2. Executes snipes when booking windows approach
3. Marks snipes as completed or failed
4. Cleans up old entries after 7 days

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

## Project Structure

```
src/
├── main.rs        # CLI and command dispatch
├── api.rs         # Perfect Gym API client
├── config.rs      # Configuration file parsing
├── email.rs       # Email notifications
├── error.rs       # Error types
├── scheduler.rs   # Auto-booking scheduler
├── snipe.rs       # Snipe logic and booking attempts
├── snipe_queue.rs # Snipe queue management
└── util.rs        # Helper functions
```

## Security Note

Your credentials are stored in plain text in `config.toml`. Keep this file secure:

```bash
chmod 600 config.toml
```

## License

MIT
