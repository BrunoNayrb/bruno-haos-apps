# Changelog

## 0.2.36

- An era station is one decade and the style on the files, such as "90s Rock" or "2000s Hip Hop". Decades are not joined into a range like "80s–2020s Rock".
- A decade with fewer than 20 songs of that style does not get its own era station. Those songs stay on the genre station.

## 0.2.35

- The style list is gone. A station is named with the tag written on the files, and it only exists when that tag is on at least 20 songs.
- A song is not moved into another style because the tag contains a word like "alternative." AudioMuse is used only for songs that have no tag.
- Era stations use that same file tag. Short decade slices of one tag are combined. A tag with fewer than 40 songs does not get an era station.


## 0.2.34

- A song stays in the style on its file. AudioMuse is only used when that tag is missing. One similar-song group can no longer rename the whole library to Alternative.
- Nu-metal and alternative metal go to Metal. Hip-hop stays Hip-Hop. Punk stays Punk. Indie, including "Alternative en Indie," goes to Indie instead of Alternative. Alternative rock stays Alternative Rock.
- A style becomes a genre station only when at least 40 songs belong to it.
- Thin decade slices of the same style are combined until the era station has enough songs. A decade that already has 80 or more stays on its own.


## 0.2.33

- Genre stations come from AudioMuse similar songs. Songs that sound alike share a station, and the station is named with one main style, such as Rock or Hip-Hop.
- Era stations use that same main style, not the detailed tag. A nu-metal song from Linkin Park lands in 2000s Alternative, not a station named after the sub-tag.
- A style only becomes a station when the library has enough songs in it. Adding music later puts the new songs into the matching genre and era station on the next library check.

## 0.2.32

- A Left Out station lists every song the scan did not put in a sound cluster. The add-on log names the artists with the most leftover songs.

## 0.2.31

- Clustering no longer collapses the library into a few giant stations. Groups are split by who is actually similar, and a broad listener tag cannot glue unrelated artists together.
- A station stays between about 20 and 70 songs when the graph supports that split. Smaller groups join only a station they strongly sound like.

## 0.2.30

- Stations come from an artist similarity graph. Similar artists and shared listener tags are the links. Clustering groups artists by who sounds like whom. The file genre tag is not used.
- A station is named from the listener tags that describe that group. Artists on the border between groups play as the fringe.

## 0.2.29

- The genre list is gone. Stations are named from the tags the artists actually have, and small groups join the artists they sound like. Nothing is forced into a fixed set of genres.

## 0.2.28

- Genre stations are specific styles. Alternative Rock, Indie Rock, Nu Metal, and Pop stay separate instead of one Alternative pile.
- A plain "Alternative" or "Rock" tag is not trusted. The artist style is used when the file tag is that generic. Soundtrack and other junk tags are ignored.
- A style with fewer than 20 songs is folded into the closest style. Reggae and Soundtrack stations are not created.

## 0.2.27

- A song keeps the genre on the file. ListenBrainz is used only when that tag is blank or junk.
- Decade stations are created at 25 songs, so slices like 2000s Rock, 2010s Rock, and 2010s Hip-Hop can appear.
- An artist station is created at 60 songs. If the similar artists are not in the library, the station fills up with other songs from that same sound. A second station is still skipped when it would repeat one that already exists.

## 0.2.26

- Americana and indie folk are no longer treated as country. Bands like The Lumineers go to Alternative.

## 0.2.25

- Artist stations are built around major artists and the artists that sound like them. A station is created only when that group has at least 100 songs. A second station is not created when it would be the same songs again.
- Adding enough music later creates the station on the next library check. No manual rescan is required.

## 0.2.24

- Genre stations use broad styles. Hip-Hop and Rap are one station. Alternative stays Alternative instead of collecting every nearby style.
- Decade stations such as "90s Rock" and "2000s Alternative" are added when that decade has at least 40 songs. Those songs also stay on the genre station.
- Artist stations mix in similar-sounding artists from Navidrome, not only songs from the same genre.
- Navidrome is checked every 10 minutes. New albums are picked up without a manual rescan.

## 0.2.23

- The admin scan line updates when the scan finishes. It says how many tracks were indexed and how many stations were created, instead of staying on "scanning the library."

## 0.2.22

- Style stations stay separate. Alternative Rock, Metal, Punk, and the rest are no longer folded into one Rock station.
- A style is merged only when it has fewer than 25 songs, and only into a station of the same style. Anything under 15 songs can fold one step into a neighbor. Songs are still all included.

## 0.2.21

- Stations are built from ListenBrainz artist styles, not the tags stored in the files. Every song stays with its artist, so nothing is left out.
- A style with fewer than 100 songs is folded into the nearest style. The result is a handful of stations, not one per artist.
- An artist station is still created when that artist has at least 20 songs.

## 0.2.20

- Repeat stays highlighted correctly after a reload. It is on unless you turn it off.

## 0.2.19

- Repeat is on the player, after shuffle. On, the station starts again when it reaches the end. With shuffle on, it reshuffles. Off, playback stops at the last song.

## 0.2.18

- Genre stations need at least 100 songs. A style that is smaller rolls into the nearest related family instead of becoming its own short station.
- A station no longer pulls in an unrelated genre because two artists were marked similar. Extra songs stay in the same family, and they are capped.
- Artist stations are created only when that artist has at least 20 songs. The extras are similar artists from the same style, not the whole genre.
- The add-on configuration no longer ships with a Navidrome address or username. Those fields start blank.

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

- Navidrome login errors now name the account that was rejected, without writing the password.

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
