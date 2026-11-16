#!/usr/bin/with-contenv bashio

export DATA_DIR="/data"
export STATIC_DIR="/app/static"
export INGRESS_PORT="8099"
export PUBLIC_PORT="8096"

export DEFAULT_ND_URL=$(bashio::config 'navidrome_url')
export DEFAULT_ND_USER=$(bashio::config 'navidrome_admin_user')
export DEFAULT_ND_PASS=$(bashio::config 'navidrome_admin_pass')
export DEFAULT_FRINGE=$(bashio::config 'fringe_percentage')
export DEFAULT_COOLDOWN=$(bashio::config 'artist_cooldown')
export DEFAULT_MIN_TRACKS=$(bashio::config 'min_station_tracks')

bashio::log.info "Starting NaviRadio (HAOS Admin Ingress: ${INGRESS_PORT}, Public Listener: ${PUBLIC_PORT})..."
exec /usr/local/bin/ha-naviradio
