# Getting help

Having trouble? Start with the quick checks below. If the problem continues,
the Support tab can create a report you can attach to a GitHub issue.

## First check: Setup Readiness

Open **Settings → Setup** and look at **Setup Readiness** near the bottom. A
ready setup should show:

1. Rocket League was found.
2. The Stats API is enabled.
3. Rocket League was restarted after the last change.
4. The overlay is connected.
5. New game data is arriving.

Just enabled or repaired the Stats API? Close and restart Rocket League. The
game only loads `DefaultStatsAPI.ini` when it starts; restarting the overlay is
not enough.

## Common fixes

### The app crashes or closes unexpectedly

Crash logging is automatic and does not require `--debug`. In the app data
folder listed below, open `logs` (or copy the folder path from **Settings →
Support**):

- Windows: `%APPDATA%\RL-Platform-Overlay\logs`
- Linux: `$XDG_CONFIG_HOME/rl-platform-overlay/logs` or
  `~/.config/rl-platform-overlay/logs`

Send `crash.log`, `app.log`, and `app.previous.log` when available, along with
what you were doing when the crash happened. `crash.log` records the latest Rust
panic, including version, time, thread, source location, and a forced backtrace.
It is preserved when the app restarts. Runtime logs include startup progress and
renderer errors and rotate at 2 MiB, retaining one previous file.

On Windows, also include `native-crash.log` if it is nonempty. This records
unhandled Windows exceptions (including some access violations) with the fault
code/address, thread, and time since startup. Older native reports rotate into
`native-crash.previous.log`. The handler writes to a file opened at startup and
avoids allocating memory or taking the regular logging lock while reporting a
fault. Windows Error Reporting still handles the exception normally.

The UI records runtime health every 30 seconds in `app.log`, including overlay,
dashboard, connection, and replay-upload state. Windows entries also include
working-set memory, private memory, and process handle count. These measurements
help investigate failures that happen after extended use.

Review these files before sharing: error messages can contain paths or player
details. They are separate from redacted diagnostics and are never uploaded
automatically. Release backtraces may have limited symbol information. Native
faults that bypass the exception filter, forced termination, and out-of-memory
aborts may leave only the runtime log; a missing crash report does not mean the
app closed normally.

### The overlay will not connect

1. Make sure Rocket League is running.
2. In **Setup**, click **Auto-detect** and check the selected game folder.
3. Make sure the Stats API is enabled with a packet rate above zero. `15` or
   `30` is a good choice for most players.
4. Restart Rocket League.
5. Check Setup Readiness again. It is normal to see no live events while you
   are still in a menu; enter Free Play or a match and check again.

If it still does not connect, open **Settings → Support**, click **Refresh
Preview**, and use **Copy Redacted Diagnostics** when filing your report.

### The game mode, teams, or match state is wrong

As soon as you notice the problem, open **Settings → Support** and click **Save
Recent Game API Log**. The app keeps up to two minutes of recent events in
memory, so you do not need to start recording before the problem happens.

When you report it, attach the generated `rl_stats_issue_log_*.txt` file and
tell us:

- what the overlay showed;
- what you expected to see;
- the playlist or private-match settings;
- whether you joined late, replaced a bot, entered overtime, or watched a replay;
- roughly when you noticed the problem.

The file includes the mode and evidence source selected by the app. It keeps
one `UpdateState` sample per second and all the less-frequent game events. It
is not written to disk until you click the save button.

### Ranks or MMR are missing

- Check that lobby ranks are enabled in the overlay or dashboard settings.
- Check that the player’s platform and account ID are correct.
- Use **Refresh** in the local MMR panel if you changed accounts.
- Some profiles may return no playlist data until Rocket League has published
  current skill information for that account.

If the wrong account is shown, send redacted diagnostics first. Describe the
expected platform without posting an account ID unless support specifically
needs it.

### The HUD is in the wrong place

Use **Arrange HUD** from the launch controls. Move the panels, then choose
**Done** to keep the layout or **Cancel** to undo the change. **Reset All** puts
every movable panel back in its default position.

## Which report should I send?

### Redacted diagnostics (recommended)

1. Open **Settings → Support**.
2. Click **Refresh Preview**.
3. Review the text shown in the preview.
4. Click **Copy Redacted Diagnostics**.

This report keeps useful connection, setup, session, player-stat, replay, and
runtime details while hiding local paths, names, account IDs, match/replay IDs,
filenames, detailed errors, upload events, and recent debug-log contents. API
keys are never included.

### Identifiable diagnostics

Turn on **Include identifiable details** only if the redacted report is not
enough—for example, when the problem depends on a particular path, replay, or
account. Review the warning and preview before copying. API keys are still
excluded.

### Recent Game API log

Raw API events can contain player names, account IDs, and match IDs. Share this
file privately when possible and attach it only when raw event evidence is
needed. The file is saved in the app data folder's `captures` directory:

- Windows: `%APPDATA%\RL-Platform-Overlay`
- Linux and other supported systems:
  `$XDG_CONFIG_HOME/rl-platform-overlay` or
  `~/.config/rl-platform-overlay`

### Debug logging

Debug logging is optional. Turn it on only while reproducing a hotkey or HUD
problem, then turn it off again. It can record keyboard key names and input or
overlay state changes while the app is running. Redacted diagnostics leave out
the recent debug log; identifiable diagnostics can include its latest lines.

## What makes a useful issue?

Please include:

- the app version;
- your Windows or Linux version, plus your display setup when relevant;
- short steps to reproduce the problem;
- what you expected and what actually happened;
- whether it happens every time;
- a screenshot with unrelated personal information removed;
- redacted diagnostics, unless support needs identifiable details or a raw API log.

Never include Ballchasing tokens, passwords, or unrelated logs. You can report
problems on the [GitHub issue tracker](https://github.com/Lucashamburguru/RL-Platform-Overlay/issues).

## Developer-only capture

Developers who need to start a capture before reproducing an issue can launch
the app with `--debug` and use **Debug → Stats API Capture**, or run:

```bash
cargo run --locked --bin debug_game_output -- --seconds 30 --output rl_game_output_debug.txt
```

These captures contain raw identifiable events and should be handled like the
one-click recent Game API log.
