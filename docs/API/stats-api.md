# Rocket League Stats API

This document outlines capabilities of the Rocket League Game Data API. First, players must ask the game to enable this feature by editing their DefaultStatsAPI.ini, explained below. Once active, this feature will open a web socket on the player's machine that emits gameplay data and events. Third party programs can ingest this data to power a variety of applications, such as custom broadcaster HUDs.

## Overview

The Stats API broadcasts JSON messages over a local socket while a match is in progress. Messages are sent both at a configurable periodic rate and when specific match events occur. Event data is always emitted on the same tick that the event occurs, regardless of the user's PacketSendRate.

**Note:** All configuration must be done before the client starts — changes to the ini while the client is running require a restart.

Field visibility:

* Fields marked CONDITIONAL are only present when relevant.
* Fields marked SPECTATOR are only present if the client is spectating or on the player's team.

## Configuration

Edit `<Install Dir>\TAGame\Config\DefaultStatsAPI.ini` before launching the client.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| PacketSendRate | float | 0 (disabled) | Number of UpdateState packets broadcast per second. Must be > 0 to enable the websocket. Capped at 120. |
| Port | int | 49123 | Local port the socket listens on. |

## Message Format

Every message follows this envelope structure:

Copy

```
{
  "Event": "EventName",
  "Data":  { /* event-specific payload */ }
}
```

Expand AllCollapse All

## Tick

### `UpdateState`▾

Sent X amount of times per second based on the player's PacketSendRate preference.

#### Example

Copy

```
{
  "Event": "UpdateState",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "Players": [
      {
        "Name": "PlayerA",
        "PrimaryId": "Steam|123|0",
        "Shortcut": 1,
        "TeamNum": 0,
        "Score": 125,
        "Goals": 1,
        "Shots": 2,
        "Assists": 0,
        "Saves": 1,
        "Touches": 14,
        "CarTouches": 3,
        "Demos": 0,
        "bHasCar": true,
        "Speed": 1200,
        "Boost": 45,
        "bBoosting": true,
        "bOnGround": true,
        "bOnWall": false,
        "bPowersliding": false,
        "bDemolished": true,
        "Attacker": {
          "Name": "PlayerB",
          "Shortcut": 2,
          "TeamNum": 1
        },
        "bSupersonic": true
      }
    ],
    "Game": {
      "Teams": [
        {
          "Name": "Blue",
          "TeamNum": 0,
          "Score": 1,
          "ColorPrimary": "0000FF",
          "ColorSecondary": "0000AA"
        }
      ],
      "TimeSeconds": 180,
      "bOvertime": false,
      "Frame": 120,
      "Elapsed": 50.2,
      "Ball": {
        "Speed": 850.5,
        "TeamNum": 0
      },
      "bReplay": false,
      "bHasWinner": true,
      "Winner": "Blue",
      "Arena": "Stadium_P",
      "bHasTarget": true,
      "Target": {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      }
    }
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| Players | array | One entry per player in the match. |
| Name | string | Display name. |
| PrimaryId | string | Platform identifier in the format Platform|Uid|Splitscreen (e.g. "Steam|123|0", "Epic|456|0"). |
| Shortcut | int | Spectator shortcut number. |
| TeamNum | int | Team index (0 = Blue, 1 = Orange). |
| Score | int | Total match score. |
| Goals | int | Goals scored this match. |
| Shots | int | Shot attempts this match. |
| Assists | int | Assists earned this match. |
| Saves | int | Saves made this match. |
| Touches | int | Total ball touches. |
| CarTouches | int | Touches by the car body (not ball). |
| Demos | int | Demolitions inflicted. |
| bHasCar | bool | SPECTATORTrue if the player currently has a vehicle. |
| Speed | float | SPECTATORVehicle speed in Unreal Units/second. |
| Boost | int | SPECTATORBoost amount 0–100. |
| bBoosting | bool | SPECTATORTrue if the player is currently boosting. |
| bOnGround | bool | SPECTATORTrue if at least 3 wheels are touching the world. |
| bOnWall | bool | SPECTATORTrue if the vehicle is on a wall. |
| bPowersliding | bool | SPECTATORTrue if the player is holding handbrake. |
| bDemolished | bool | SPECTATORTrue if the vehicle is currently destroyed. |
| bSupersonic | bool | SPECTATORTrue if the vehicle is at supersonic speed. |
| Attacker | object | CONDITIONALThe player who demolished this player. Present only when demolished. |
| Name | string | Name of the player who demolished this player. |
| Shortcut | int | Spectator shortcut of the attacker. |
| TeamNum | int | Team index of the attacker. |
| Game | object | Match metadata. |
| Teams | array | One entry per team, ordered by TeamNum. |
| Name | string | Team name. |
| TeamNum | int | Team index. |
| Score | int | Team goal count. |
| ColorPrimary | string | Hex color code (no #) for the team’s primary color. |
| ColorSecondary | string | Hex color code for the team’s secondary color. |
| TimeSeconds | int | Seconds remaining in the match. |
| bOvertime | bool | True if the match is in overtime. |
| Ball | object | Current ball state. |
| Speed | float | Current ball speed in Unreal Units/second. |
| TeamNum | int | Index of the last team to touch the ball. 255 if the ball has not been touched. |
| bReplay | bool | True if a goal replay or history replay is active. |
| bHasWinner | bool | True if a team has won. |
| Winner | string | Name of the winning team. Empty string if no winner yet. |
| Arena | string | Asset name of the current map (e.g. "Stadium\_P"). |
| bHasTarget | bool | True if the client is currently viewing a specific vehicle. |
| Target | object | CONDITIONALPlayer currently being viewed. Members are an empty string or 0 if the player does not have a spectator target. |
| Name | string | Name of the player being viewed. |
| Shortcut | int | Spectator shortcut of the viewed player. |
| TeamNum | int | Team index of the viewed player. |
| Frame | int | CONDITIONALCurrent frame number if a replay is active. |
| Elapsed | float | CONDITIONALSeconds elapsed since game start if a replay is active. |

## Events

### `BallHit`▾

Sent one frame after the ball is hit.

#### Example

Copy

```
{
  "Event": "BallHit",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "Players": [
      {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      }
    ],
    "Ball": {
      "PreHitSpeed": 0,
      "PostHitSpeed": 1450.2,
      "Location": {
        "X": -512,
        "Y": 100,
        "Z": 200
      }
    }
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| Players | array | Players that hit the ball that frame. |
| Name | string | Display name. |
| Shortcut | int | Spectator shortcut. |
| TeamNum | int | Team index (0 = Blue, 1 = Orange). |
| Ball | object | Ball state at the moment of the hit. |
| PreHitSpeed | float | Ball speed before the hit (Unreal Units/second). |
| PostHitSpeed | float | Ball speed after the hit (Unreal Units/second). |
| Location | vector | World position (X, Y, Z) of the ball at impact. |
| MatchGuid | string | Only set for online or LAN matches. |

### `ClockUpdatedSeconds`▾

Sent when the in-game clock has changed.

#### Example

Copy

```
{
  "Event": "ClockUpdatedSeconds",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "TimeSeconds": 180,
    "bOvertime": false
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| TimeSeconds | int | Seconds remaining in the match. |
| bOvertime | bool | True if the game is in overtime. |
| MatchGuid | string | Only set for online or LAN matches. |

### `CountdownBegin`▾

Sent at the start of each round when the countdown starts.

#### Example

Copy

```
{
  "Event": "CountdownBegin",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `CrossbarHit`▾

Sent when the ball hits a crossbar.

#### Example

Copy

```
{
  "Event": "CrossbarHit",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "BallLocation": {
      "X": 120,
      "Y": -2944,
      "Z": 320
    },
    "BallSpeed": 870.3,
    "ImpactForce": 127.5,
    "BallLastTouch": {
      "Player": {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      },
      "Speed": 120
    }
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| BallSpeed | float | Ball speed on impact. |
| ImpactForce | float | Impact force of the ball relative to the crossbar normal. |
| BallLastTouch | object | The last touch of the ball before the crossbar hit. |
| Player | object | The player who made the last touch. |
| Name | string | Display name. |
| Shortcut | int | Spectator shortcut. |
| TeamNum | int | Team index (0 = Blue, 1 = Orange). |
| Speed | float | Speed of the ball resulting from this hit. |
| BallLocation | vector | World position (X, Y, Z) of the ball when the impact occurred. |
| MatchGuid | string | Only set for online or LAN matches. |

### `GoalReplayEnd`▾

Sent when a goal replay ends.

#### Example

Copy

```
{
  "Event": "GoalReplayEnd",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `GoalReplayStart`▾

Sent when a goal replay starts.

#### Example

Copy

```
{
  "Event": "GoalReplayStart",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `GoalReplayWillEnd`▾

Sent when the ball explodes during a goal replay. If the replay is skipped this event will not fire.

#### Example

Copy

```
{
  "Event": "GoalReplayWillEnd",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `GoalScored`▾

Sent when a goal is scored.

#### Example

Copy

```
{
  "Event": "GoalScored",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "GoalSpeed": 87.3,
    "GoalTime": 127.5,
    "ImpactLocation": {
      "X": 0,
      "Y": -2944,
      "Z": 320
    },
    "Scorer": {
      "Name": "PlayerA",
      "Shortcut": 1,
      "TeamNum": 0
    },
    "Assister": {
      "Name": "PlayerC",
      "Shortcut": 3,
      "TeamNum": 0
    },
    "BallLastTouch": {
      "Player": {
        "Name": "PlayerA",
        "Shortcut": 1,
        "TeamNum": 0
      },
      "Speed": 125
    }
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| GoalSpeed | float | Speed of the ball (Unreal Units/second) when it crossed the goal line. |
| GoalTime | float | Length of the previous round in seconds. |
| ImpactLocation | vector | World position (X, Y, Z) of the ball when the goal was scored. |
| Scorer | object | The player who scored the goal. |
| Name | string | Display name of the scorer. |
| Shortcut | int | Spectator shortcut. |
| TeamNum | int | Team index of the scorer. |
| BallLastTouch | object | The last touch of the ball before the goal. |
| Player | object | The player who made the last touch. |
| Name | string | Name of the player who last touched the ball. |
| Shortcut | int | Spectator shortcut. |
| TeamNum | int | Team index. |
| Speed | float | Speed of the ball resulting from this touch. |
| Assister | object | CONDITIONALSame shape as Scorer. Present only when an assist was recorded. |
| MatchGuid | string | Only set for online or LAN matches. |

### `MatchCreated`▾

Sent when all teams are created and replicated.

#### Example

Copy

```
{
  "Event": "MatchCreated",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `MatchInitialized`▾

Sent when the first countdown starts.

#### Example

Copy

```
{
  "Event": "MatchInitialized",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `MatchDestroyed`▾

Sent when leaving the game.

#### Example

Copy

```
{
  "Event": "MatchDestroyed",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `MatchEnded`▾

Sent when the match ends and a winner is chosen.

#### Example

Copy

```
{
  "Event": "MatchEnded",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "WinnerTeamNum": 0
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |
| WinnerTeamNum | int | Team index of the winning team. |

### `MatchPaused`▾

Sent when the game is paused by a match admin.

#### Example

Copy

```
{
  "Event": "MatchPaused",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `MatchUnpaused`▾

Sent when the game is unpaused by a match admin.

#### Example

Copy

```
{
  "Event": "MatchUnpaused",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `PodiumStart`▾

Sent when the game enters the podium state after the match ends.

#### Example

Copy

```
{
  "Event": "PodiumStart",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `ReplayCreated`▾

Sent when a replay is initialized. Does not pertain to goal replays, only replays you load via the Match History menu.

#### Example

Copy

```
{
  "Event": "ReplayCreated",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `RoundStarted`▾

Sent when the game enters the active state (after the countdown finishes).

#### Example

Copy

```
{
  "Event": "RoundStarted",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6"
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| MatchGuid | string | Only set for online or LAN matches. |

### `StatfeedEvent`▾

Sent when someone earns a stat.

#### Example

Copy

```
{
  "Event": "StatfeedEvent",
  "Data": {
    "MatchGuid": "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6",
    "EventName": "Demolish",
    "Type": "Demolition",
    "MainTarget": {
      "Name": "PlayerA",
      "Shortcut": 1,
      "TeamNum": 0
    },
    "SecondaryTarget": {
      "Name": "PlayerB",
      "Shortcut": 2,
      "TeamNum": 1
    }
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| EventName | string | Asset name of the StatEvent (e.g. "Demolish", "Save"). |
| Type | string | Localized display label for the stat (e.g. "Demolition"). |
| MainTarget | object | Player who earned the stat. |
| Name | string | Display name. |
| Shortcut | int | Spectator shortcut. |
| TeamNum | int | Team index (0 = Blue, 1 = Orange). |
| MatchGuid | string | Only set for online or LAN matches. |
| SecondaryTarget | object | CONDITIONALPlayer involved in the stat (e.g. the demolished player). Same shape as MainTarget. |

## Observed Behavior From Local Captures

These notes come from local `rl_stats_capture_*.txt` captures recorded through the overlay's debug capture tool. They describe behavior seen in practice and should be treated as parser guidance, not a replacement for the API reference above.

### Transport

The API may reject the WebSocket probe with `invalid HTTP version` and still emit valid JSON over raw TCP on the same port. The overlay supports both transport shapes.

### Envelope Shape

Captured messages used the same outer shape:

```json
{
  "Event": "UpdateState",
  "Data": "{...json string...}"
}
```

`Data` may arrive as a JSON-encoded string. Consumers should decode `Data` when it is a string before reading event fields.

### Update Frequency

With the current setup, `UpdateState` arrives at roughly 30 Hz.

Observed counts:

| Capture length | `UpdateState` frames with `Players` | Approx interval |
| --- | ---: | ---: |
| 5 seconds | 149 | 33 ms |
| 30 seconds | 898-899 | 33 ms |

`UpdateState` is a full state stream, not a semantic "roster changed" event. Avoid expensive work on every frame. Prefer comparing a roster identity signature before refreshing history, MMR, or other derived data.

### Roster Changes vs Stat Changes

Most `UpdateState` frames include a complete `Players` array. Player identity/team changes are rare, while boost/stat fields change frequently.

Observed pattern:

| Change type | Typical frequency in captures |
| --- | --- |
| Player identity/team roster changes | 0-5 per capture |
| Boost/score/touch/stat changes | dozens to hundreds per capture |

Recommended roster identity signature:

```text
normalized(platform + primary_id), team, is_local
```

Do not include boost, score, touches, demos, or speed in the signature if the goal is detecting roster membership changes.

### Local Player Detection

The local player is not guaranteed to have an explicit local-player flag. In the local captures, player entries did not include `IsLocalPlayer`, `isLocalPlayer`, or `isMe`.

Use these hints, in order:

1. cached local identity by `platform + primary_id`
2. `Game.Target.Name` and `Game.Target.TeamNum`
3. known local player name from prior frames

`Game.Target` can be present even when `bHasTarget` is true. Do not skip parsing `Target` just because `bHasTarget` is true.

### Common `UpdateState.Game` Fields

Captured `UpdateState` payloads consistently included:

```text
Teams
TimeSeconds
bOvertime
Ball
bReplay
bHasWinner
Winner
Arena
bHasTarget
Target
```

`Target` was absent on some replay/end-screen frames, especially when `bHasTarget` was false.

### Common Player Fields

Captured player entries commonly included:

```text
Name
Shortcut
TeamNum
PrimaryId
Score
Goals
Shots
Assists
Saves
Touches
CarTouches
Demos
```

Movement/car fields are conditional. Examples seen in captures:

```text
bHasCar
Speed
Boost
bOnGround
bBoosting
bSupersonic
bOnWall
bPowersliding
bDemolished
```

Bots/private match filler players may use `PrimaryId = "Unknown|0|0"` and `platform = "Unknown"` after parsing. Do not treat those as durable player identities.

### Match-End Ordering

Typical end-of-match ordering seen in captures:

```text
StatfeedEvent
GoalScored
GoalReplayEnd
ClockUpdatedSeconds
MatchEnded
UpdateState with Game.bHasWinner = true
...
PodiumStart
more final UpdateState frames
```

`MatchEnded` can arrive before the first `UpdateState` frame that has `bHasWinner = true`.

Result fields differ by event:

| Source | Result fields |
| --- | --- |
| `MatchEnded` | `WinnerTeamNum` |
| `UpdateState.Game` | `bHasWinner`, `Winner` as `"Blue"` or `"Orange"` |

Consumers should support both.

### Ready-Up / Rematch GUID Quirk

On the end screen, pressing ready can produce a new `MatchGuid` while the API is still emitting the previous final result state.

Observed shape:

```text
old MatchGuid -> bHasWinner=true, Winner=Blue, final score
new MatchGuid -> bHasWinner=true, Winner=Blue, same final score
```

Do not count the second frame as a new match result. Dedupe completed results using more than `MatchGuid`; include a short-lived fingerprint such as:

```text
mode, local team, blue score, orange score, result
```

The overlay currently stores the last recorded result fingerprint for this reason.

### Reset / Teardown Events

`PodiumStart` is not a reliable reset boundary. Final winner frames continue after it.

`MatchDestroyed` is a stronger teardown signal. In a ready-up capture, the sequence was:

```text
MatchDestroyed with non-empty MatchGuid
ClockUpdatedSeconds with empty MatchGuid
MatchCreated with empty MatchGuid
UpdateState with prior/new non-empty MatchGuid
```

Consumers should handle stale frames after reset events.

### Freeplay Shape

Freeplay/training can appear as:

```text
MatchGuid = ""
Players length = 1
Game present
bHasWinner = false
```

The overlay treats empty `MatchGuid` plus one player as `Freeplay` instead of inferring `1v1`.

### Observed Event Names

The local captures included these event names:

```text
UpdateState
ClockUpdatedSeconds
BallHit
StatfeedEvent
GoalScored
GoalReplayStart
GoalReplayEnd
RoundStarted
MatchEnded
PodiumStart
CountdownBegin
ReplayPlaybackStart
ReplayWillEnd
ReplayPlaybackEnd
MatchInitialized
MatchDestroyed
MatchCreated
```

Not every event appears in every capture. Consumers should ignore unknown events and continue parsing `UpdateState`.

## Real-World Capture Samples

The following are raw JSON payload examples extracted from the local capture directory.

### 1. `StatfeedEvent` (e.g. Save, Shot, Assist, Demolish)
This event is emitted whenever a player earns a match stat.

```json
{
  "Event": "StatfeedEvent",
  "Data": {
    "MatchGuid": "381400EC11F165783FBDBBB93AC558AA",
    "EventName": "Save",
    "Type": "Save",
    "MainTarget": {
      "Name": "Jester",
      "Shortcut": 6,
      "TeamNum": 1
    }
  }
}
```

```json
{
  "Event": "StatfeedEvent",
  "Data": {
    "MatchGuid": "C7C704F611F16194479C8AA3417B4FD5",
    "EventName": "Shot",
    "Type": "Shot on Goal",
    "MainTarget": {
      "Name": "ok zeon",
      "Shortcut": 6,
      "TeamNum": 1
    }
  }
}
```

### 2. `GoalScored`
Emitted immediately when a goal is recorded, before the replay sequence starts.

```json
{
  "Event": "GoalScored",
  "Data": {
    "MatchGuid": "381400EC11F165783FBDBBB93AC558AA",
    "GoalSpeed": 83.01599884033203,
    "GoalTime": 31,
    "ImpactLocation": {
      "X": -713.93,
      "Y": 5275.01,
      "Z": 302.96
    },
    "Scorer": {
      "Name": "cyberPeng",
      "Shortcut": 5,
      "TeamNum": 1
    },
    "Assister": {
      "Name": "Jester",
      "Shortcut": 6,
      "TeamNum": 1
    },
    "BallLastTouch": {
      "Player": {
        "Name": "cyberPeng",
        "Shortcut": 5,
        "TeamNum": 1
      },
      "Speed": 87.28392028808594
    }
  }
}
```

### 3. Platform Prefix Variations (`PrimaryId` field)
Observe that the platform prefixes returned in the raw socket stream are case-sensitive and vary from standard representations:
* **PlayStation**: Emits `"PS4"` (all uppercase) instead of `"Ps4"`, e.g., `"PrimaryId": "PS4|8610596688484080466|0"`.
* **Xbox**: Emits `"XboxOne"` (lowercase `b`, uppercase `O`) instead of `"XBoxOne"`, e.g., `"PrimaryId": "XboxOne|2535439644603854|0"`.
* **Bots**: Emits `"Unknown|0|0"` or `"Bot|0|0"`.

To avoid parsing mismatches, prefix matching must be normalized case-insensitively (e.g. converting to lowercase before evaluating).

