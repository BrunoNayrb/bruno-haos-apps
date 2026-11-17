# Changelog

## 0.2.17

- Stop and previous swapped places. Stop is now to the left of previous.
- Stopping no longer leaves the album art sitting on the page after the player closes.

## 0.2.16

- The large player uses a blurred copy of the cover as its background.
- Up next shows the following songs under the volume slider. Choosing one plays it.
- The large volume slider stays centered. Mute sits to its left.

## 0.2.15

- Mute sits with the volume slider, not in the play controls.
- The play button is centered, with the same number of buttons on each side. The large player centers that row, then the mute button and slider under it.

## 0.2.14

- Click the album art in the player to open a large player. The art grows into the center and the screen slides up. The controls under it are larger.
- The chevron slides it back down. Stop closes it too.

## 0.2.13

- The player has a mute button with the other controls, and a volume slider to the right of them.
- Volume is remembered in the browser.

## 0.2.12

- The public site will not play a song or show cover art until you sign in. A direct link to a track no longer works on its own.
- Sign-in lasts 30 days and does not interrupt a station that is already playing. Log Out ends it.

## 0.2.11

- The Navidrome password is no longer sent to the browser or written to the add-on log.
- Wrong listener passwords are limited by network address, not by account. Five misses pauses that address for about a minute, then five minutes, then fifteen. A correct password still signs in, and the account is never locked.

## 0.2.10

- The play triangle fills the round button instead of sitting small inside it.
- The bottom bar stays hidden until a song is playing or paused. There is no empty cover while nothing is playing.
- Stop ends playback and hides the bar.

## 0.2.9

- The bottom bar stays hidden until a song is playing or paused.
- Stop ends playback and hides the bar.
- The play button is a filled circle with the icon centered.

## 0.2.8

- The bottom play button is round, and the player controls sit in one group with a progress bar.
- Each station in the sidebar has a heart at the end of its name.
- Genre Stations are listed above Artist Stations.
- Shuffle is on by default. It spreads songs so one artist does not play as a block.
- The station page has a Shuffle button that starts playback immediately. The player has a shuffle button too.

## 0.2.7

- Stations are listed in a sidebar under Favorites, Artist Stations, and Genre Stations.
- A favorited station stays in its own category and is also listed under Favorites.
- Choosing a station shows its songs on the right. The round play button starts the station. Choosing a song starts playback there.
- The player uses previous, play/pause, and next. Play is shown while stopped or paused. Pause is shown while a song is playing.
- The heart on the player favorites the current station. It is an outline until favorited, then filled.
- The list button opens the radio queue.

## 0.2.6

- The default Navidrome username is `badlonegamer`. The previous default, `badonegamer`, is not an account on the server.

## 0.2.5

- Navidrome login attempts are written to the add-on log. The password itself is not written.

## 0.2.4

- The library scan signs in through Navidrome's `/auth/login` endpoint and then uses the token Navidrome returns.

## 0.2.3

- Rescan uses the Navidrome service username and password from Admin Settings. The listener sign-in is not used for the scan.
- A password is stored only after Navidrome accepts it.

## 0.2.2

- Library scans walk albums instead of calling `search3` with an empty query. That call returns no songs, so earlier scans indexed 0 tracks.

## 0.2.1

- The public listener port moved from 8096 to 8093 so it does not collide with Emby. Ingress stays on 8099.

## 0.2.0

- First release in this repository.
