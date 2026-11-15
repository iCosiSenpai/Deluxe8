# Online play

Mario Kart 8 Deluxe has three ways to race other people besides split screen: **online play** (the console's online
service), **wireless play** (local wireless, up to 8 consoles) and a hidden **LAN play** mode (L + R + left stick on the
title menu, up to 12 players). Deluxe8 never talks to Nintendo's servers. It offers two things instead:

| | Nextendo Network (Online engine) | Rooms (Emulator engine) |
|---|---|---|
| Game mode | *Online play*: worldwide/regional races, tournaments, friends, time-trial ghosts | *Wireless play* and *LAN play* |
| Runs in | Ryujinx Nextendo, downloaded by the launcher | Deluxe8's Emulator engine (`deluxe8-engine`) |
| Plays with | Ryujinx Nextendo, Citron Nextendo, consoles with Nextendo's Prelude homebrew (custom firmware) | Deluxe8, Eden, Citron, suyu, Sudachi (yuzu-family room protocol, `network_version` 1) |
| Account | free Nextendo account | a driver name |
| Servers | Nextendo Network (community-run) | the public room lobby, or a room you host |

Everyone must run the **same game update** (4.0.1) in both cases.

## Nextendo Network

[Nextendo Network](https://nextendo.network) is an independent, community-run replacement of the console's online
service: accounts, friends, presence, matchmaking and the per-game servers (MK8D: races, rooms, tournaments, ghosts).
Its client side lives in two emulator builds by the Nextendo team, Ryujinx Nextendo and Citron Nextendo, and in the
Prelude homebrew for consoles. Deluxe8 uses **Ryujinx Nextendo** because its releases are versioned (the Citron builds
are nightly and their downloads get replaced).

What the launcher does (`apps/launcher/src-tauri/src/nextendo.rs`):

1. Downloads Ryujinx Nextendo v1.10.6 (Windows zip / Linux tar.gz), pinned by URL, size and SHA-256 in
   `components.rs`, into `<data>/engines/online/v1.10.6`.
2. Keeps Ryujinx's data in `<data>/online` (`-r <dir>`): `system/prod.keys` and `title.keys` (copies), the firmware as
   `bis/system/Contents/registered/<id>.nca/00` (hard links to the NCAs the preparation installed), and
   `games/0100152000022000/updates.json` + `dlc.json` pointing at the converted update and DLC. It marks Ryujinx's
   first-start wizard and the current release notes as done (`nextendo_setup_done`, `nextendo_patchnote_version`):
   the launcher already did that work.
3. *Sign in* opens Ryujinx Nextendo without a game; the player links the account under *Settings › Nextendo*. The
   account file (`nextendo_account.txt`, protected by Windows on Windows) holds a login token: Deluxe8 only checks
   that it exists.
4. *Play online* starts `Ryujinx -r <data>/online [--fullscreen] --docked-mode --system-language <lang>
   --system-region <region> <base.nsp>`. Before an online title starts, Ryujinx Nextendo checks the account, the
   server and the game version; without an account it offers to start offline.

Verified on Windows (RTX 5070 Ti): firmware 22.5.0 recognised, update 4.0.1 and the Booster Course Pass loaded, the
game boots; update 4.0.1 passes Nextendo's version check. Races on the network need the player's own account.

## Rooms

The Emulator engine uses the yuzu-family **rooms**: every player connects to a room server, which relays local-wireless
(LDN) and LAN packets between the emulated consoles, so the game believes the others are in the same room. The protocol
(`src/network` in Eden) is the same in Citron and suyu, so their players join Deluxe8 rooms and the other way round.

- **Browse**: the Online page lists public rooms from the lobby (default: the community lobby Eden uses,
  `api.ynet-fun.xyz`) filtered to MK8D, with the advertised game version.
- **Join**: Deluxe8 starts the engine with `-m nickname:password@address:port` (`nickname@address:port` for a room
  without a password; the password never reaches the logs). Nicknames: 4–20 characters, letters, digits, space,
  `. _ -`.
- **Host**: Deluxe8 starts `deluxe8-room` (Eden's dedicated room server) on UDP port 24872 and, if your router supports
  UPnP, opens the port automatically (and closes it when the room stops or the launcher exits). Rooms are private by
  default: share your address with friends. The advertised game name includes the version (`Mario Kart 8 Deluxe 4.0.1`)
  so mismatched players see it before joining.

Tips: prefer wired connections and a nearby host (the room relays traffic, adding about one round trip), and warm the
shader cache with a few offline races (a stutter on one player can drop the session).

### Self-hosting a room server

`deluxe8-room` is Eden's `eden-room`. On a VPS:

```sh
deluxe8-room --room-name "Grand Prix" --port 24872 --max-members 12 \
  --preferred-game "Mario Kart 8 Deluxe 4.0.1" --preferred-game-id 0100152000022000 --password "secret"
```

Open UDP 24872 on the firewall. Without `--token` the room is not announced in the public lobby; players join by
address.

## Not supported (and why)

- **Nintendo's online service**: never. Deluxe8 does not connect to Nintendo's servers.
- **Native engine online**: the recompiled engine comes from Racing 8 DX Recomp, whose build never starts its network
  stack (it keeps yuzu's room code and `--multiplayer` option but never calls `Network::Init`). Enabling it means
  shipping our own build of that engine; until then, online play uses the other two engines.
- **Rooms with consoles**: possible only with homebrew on the console (ldn_mitm + switch-lan-play); not integrated.
  Consoles meet Deluxe8 players on the Nextendo Network instead.

## References

- Nextendo Network: https://nextendo.network, sources at https://git.nextendo.network/NextendoNetwork (servers under the
  PolyForm Shield licence, Ryujinx Nextendo under MIT, Citron Nextendo under the GPL)
- Eden room server and lobby protocol: `src/dedicated_room`, `src/web_service/announce_room_json.cpp` in
  https://git.eden-emu.dev/eden-emu/eden
