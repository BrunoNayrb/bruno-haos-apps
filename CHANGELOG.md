# Changelog

All notable changes to Bruno's HAOS Apps are recorded here. The newest release is at the top.

Add-on versions are the `version` field in each add-on's `config.yaml`. Home Assistant shows that version on the add-on page.

## NaviRadio

### 0.2.10 — 2026-11-17

- The play triangle fills the round button instead of sitting small inside it.
- The bottom bar stays hidden until a song is playing or paused. There is no empty cover while nothing is playing.
- Stop ends playback and hides the bar.

### 0.2.9 — 2026-11-17

- The bottom bar stays hidden until a song is playing or paused.
- Stop ends playback and hides the bar.
- The play button is a filled circle with the icon centered.

### 0.2.8 — 2026-11-17

- The bottom play button is round, and the player controls sit in one group with a progress bar.
- Each station in the sidebar has a heart at the end of its name.
- Genre Stations are listed above Artist Stations.
- Shuffle is on by default. It spreads songs so one artist does not play as a block. The station page has a Shuffle button that starts playback immediately. The player has a shuffle button too.

### 0.2.7 — 2026-11-17

- Stations are listed in a sidebar under Favorites, Artist Stations, and Genre Stations.
- A favorited station stays in its own category and is also listed under Favorites.
- Choosing a station shows its songs on the right. The round play button starts the station. Choosing a song starts playback there.
- The player uses previous, play/pause, and next. Play is shown while stopped or paused. Pause is shown while a song is playing.
- The heart on the player favorites the current station. It is an outline until favorited, then filled.
- The list button opens the radio queue.

### 0.2.6 — 2026-11-17

- The default Navidrome username is `badlonegamer`. The previous default, `badonegamer`, is not an account on the server, which is why login returned "data not found".

### 0.2.5 — 2026-11-17

- Navidrome login attempts are written to the add-on log. The password is not written. The log includes the username, URL, HTTP status, rate-limit headers, and the shape of the password (length and a short hash).

### 0.2.4 — 2026-11-17

- The library scan signs in through Navidrome's own `/auth/login` endpoint, the same one the website uses, and then uses the token Navidrome returns.

### 0.2.3 — 2026-11-17

- Rescan uses the Navidrome service username and password from Admin Settings. The listener sign-in is separate and is not used for the scan.
- A password is stored only after Navidrome accepts it.

### 0.2.2 — 2026-11-17

- Library scans walk albums instead of calling `search3` with an empty query. That call returns no songs, so earlier scans indexed 0 tracks.

### 0.2.1 — 2026-11-16

- The public listener port moved from 8096 to 8093 so it does not collide with Emby. The Home Assistant ingress port stays 8099.

### 0.2.0 — 2026-11-16

- First release in this repository.
- NaviRadio, Soulbeet, and Soulbeet Bulk live in one Home Assistant add-on repository.
- The image is published to `ghcr.io/brunonayrb/naviradio`.

## Soulbeet

Brought into this repository on 2026-11-16 at version `0.7.1-localdb3`. Older notes stayed in `https://github.com/BrunoNayrb/ha-soulbeet`.

- Runs the official Soulbeet app.
- Beets databases are stored on the Home Assistant disk under `/data/beets_dbs/`.
- Host port 9765.

## Soulbeet Bulk

Brought into this repository on 2026-11-16 at version `0.1.26`. Older notes stayed in `https://github.com/BrunoNayrb/ha-soulbeet`.

- Soulbeet fork with the local database fix and CSV import.
- Host port 9766, so it can run beside Soulbeet.
- The image is published to `ghcr.io/brunonayrb/soulbeet-bulk`.

## Adding an entry

When an add-on version changes, add a section above the previous one for that add-on. Include the version, the date, and what changed for someone updating Home Assistant. Do not list commits that were only a failed build.
