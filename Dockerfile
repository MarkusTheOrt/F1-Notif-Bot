FROM rust:1.67.1 as build-stage

WORKDIR /app

COPY . .
RUN apt-get install libc6-dev

RUN RUSTFLAGS="-C target-feature=+crt-static" cargo install --path .

FROM debian:buster-slim as runner

RUN apt-get update && rm -rf /var/lib/apt/lists/*
COPY --from=build-stage /usr/local/cargo/bin/f1-notif-bot /usr/local/bin/f1-notif-bot
CMD ["f1-notif-bot"]