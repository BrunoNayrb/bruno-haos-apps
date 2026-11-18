# Bruno's HAOS Apps

Home Assistant add-on repository for the add-ons I run on HAOS.

Add this repository in the add-on store:

```text
https://github.com/BrunoNayrb/brunos-haos-apps
```

Reload it. All three add-ons show up in one section named **Bruno's HAOS Apps**.

| Folder | Add-on | Image |
|--------|--------|--------|
| `naviradio/` | NaviRadio | `ghcr.io/brunonayrb/naviradio` |
| `soulbeet/` | Soulbeet | Built by Home Assistant from this folder |
| `soulbeet-bulk/` | Soulbeet Bulk | `ghcr.io/brunonayrb/soulbeet-bulk` |

Soulbeet and Soulbeet Bulk can be installed together. Bulk uses host port 9766 so it does not collide with Soulbeet on 9765.

Soulbeet is Terry Raimondo's project: [terry90/soulbeet](https://github.com/terry90/soulbeet). [BrunoNayrb/soulbeet](https://github.com/BrunoNayrb/soulbeet) is a fork of that repository. These folders only package it for Home Assistant. The Soulbeet add-on runs his published image. Soulbeet Bulk builds from that fork.

Changes are listed in each add-on's `CHANGELOG.md`. That is the file Home Assistant shows on the add-on page. The repository index is [CHANGELOG.md](CHANGELOG.md).

## After you switch

If Home Assistant still has the old repository URLs, remove them so you are not loading the same add-on twice:

- `https://github.com/BrunoNayrb/ha-naviradio`
- `https://github.com/BrunoNayrb/ha-soulbeet`

Already installed add-ons keep running. Their images did not change.
