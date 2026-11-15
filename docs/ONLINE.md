# Online play

## How it works

Mario Kart 8 Deluxe has three multiplayer modes besides split-screen: online (Nintendo Switch Online), **local
wireless** (up to 8 consoles) and a hidden **LAN mode** (L + R + left stick on the title menu, up to 12 players).
Deluxe8 never talks to Nintendo's servers. The Compat engine uses Eden's **rooms**: every player connects to a room
server, and the room relays local-wireless (LDN) and LAN packets between the emulated consoles, so the game believes
the other players are in the same room.

- **Browse**: the Online page lists public rooms from the lobby (default: the community lobby Eden uses,
  `api.ynet-fun.xyz`) filtered to MK8D, with the advertised game version.
- **Join**: Deluxe8 starts the engine with `-m nickname:password@address:port` (`nickname@address:port` for a room
  without a password; the password never reaches the logs). Nicknames: 4–20 characters, letters,
  digits, space, `. _ -`.
- **Host**: Deluxe8 starts `deluxe8-room` (Eden's dedicated room server) on UDP port 24872 and, if your router supports
  UPnP, opens the port automatically. Rooms are private by default: share your address with friends. The advertised
  game name includes the version (`Mario Kart 8 Deluxe 4.0.1`) so mismatched players see it before joining.

## Rules that matter

- **Everyone must run the same game update.** Local-wireless sessions between different versions desync or refuse to
  connect. The room browser shows the version and the "Solo versione 4.0.1" filter is on by default.
- Prefer wired connections and a nearby room host; the room relays traffic, adding roughly one round trip.
- Pre-warm the shader cache with a few offline races: compilation stutter on one player can drop the session.

## Self-hosting a room server

`deluxe8-room` is Eden's `eden-room`. On a VPS:

```sh
deluxe8-room --room-name "Gran Premio" --port 24872 --max-members 12 \
  --preferred-game "Mario Kart 8 Deluxe 4.0.1" --preferred-game-id 0100152000022000 --password "segreta"
```

Open UDP 24872 on the firewall. Without `--token` the room is not announced in the public lobby; players join by
address.

## What is not supported (and why)

- **Nintendo Switch Online**: never. Connecting emulators to Nintendo's services risks account and console bans and
  is not something this project does.
- **Native engine online**: the static recompilation engine does not have a network backend yet (upstream roadmap).
  Rooms work with the Compat engine.
- **Cross-play with real consoles**: possible only with homebrew on the console (ldn_mitm + switch-lan-play, or
  ryu_ldn_nx for RyuLDN servers); not integrated in Deluxe8.

## References

- Eden room server and lobby protocol: `src/dedicated_room`, `src/web_service/announce_room_json.cpp` in
  https://git.eden-emu.dev/eden-emu/eden
- Ryubing LDN server (alternative ecosystem, MIT): https://github.com/Ryubing/LdnServer
- ryu_ldn_nx (console bridge for RyuLDN): https://github.com/Ethiquema/ryu_ldn_nx
