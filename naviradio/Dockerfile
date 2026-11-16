FROM rust:1-alpine3.23 AS builder

RUN apk add --no-cache musl-dev build-base pkgconfig

WORKDIR /app
COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release

FROM ghcr.io/home-assistant/base:3.23

RUN apk add --no-cache ca-certificates tzdata libgcc

WORKDIR /app
COPY --from=builder /app/target/release/ha-naviradio /usr/local/bin/ha-naviradio
COPY static /app/static
COPY run.sh /run.sh

RUN chmod a+x /run.sh /usr/local/bin/ha-naviradio

CMD [ "/run.sh" ]
