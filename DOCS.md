# Soulbeet add-on documentation

Soulbeet is Terry Raimondo's application: [terry90/soulbeet](https://github.com/terry90/soulbeet), MIT license. [BrunoNayrb/soulbeet](https://github.com/BrunoNayrb/soulbeet) is a fork of that repository. These add-ons package it. They are not a separate project.

**Soulbeet** runs his published image and stores the beets databases on the Home Assistant disk.

**Soulbeet Bulk** runs the fork, which adds CSV import. It uses host port 9766 so it can run beside Soulbeet on port 9765.

## Folders

Nothing in the fork assumes a particular disk layout.

- `download_path` and `music_base` are set on the add-on Configuration tab.
- The libraries themselves are added in the Soulbeet settings. Bulk import shows those libraries in a menu and sends downloads to the one you pick.
- Beets database files are redirected to `/data/beets_dbs/` on the Home Assistant disk. The music files stay where you put them.

| Path inside the add-on | Purpose |
|------------------------|---------|
| The download path you set | slskd downloads |
| Each library you add | Music files, and a bulk-import target |
| `/data/beets_dbs/` | Beets library databases |
| `/data/soulbeet.db` | Soulbeet application database |
| `/data/bulk_history.json` | Bulk import history |
| `/data/secret_key` | Web UI secret, created on first start |

## CSV import

```csv
title,album,artist
Alive,Ten,Pearl Jam
