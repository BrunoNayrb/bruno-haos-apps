# Changelog

## 0.1.31

- Export failed tracks repairs a title that was cut apart at a comma, such as "Money Game, Pt. 3" and "Fragile (feat. Kendrick Lamar, ¡MAYDAY! & Kendall Morgan)".
- A number that is part of the real title or album, such as Rush "2112", is left as it is.

## 0.1.30

- A CSV import keeps commas that are inside quotes, so a title like "Money Game, Pt. 3" stays one song.

## 0.1.29

- Bulk import can export the failed tracks as `failed-tracks.csv`. The file downloads in the browser and uses the same title, album, and artist columns as an import.

## 0.1.28

- A live recording is skipped when "live" is in the Soulseek folder or filename, not only in the song title.
- The published image is `ghcr.io/brunonayrb/soulbeet-bulk-ha`. The old image name could not be updated after the repository move.

## 0.1.26

- Brought into Bruno's HAOS Apps on 2026-11-16.
- Soulbeet fork with the local database fix and CSV import.
- Host port 9766, so it can run beside Soulbeet.
- The image is published to `ghcr.io/brunonayrb/soulbeet-bulk`.
