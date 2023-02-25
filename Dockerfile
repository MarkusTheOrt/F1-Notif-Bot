FROM rust:latest as build-stage

WORKDIR /app

COPY . .

RUN rustup toolchain install nightly
RUN cargo +nightly install --path .

FROM debian:buster-slim

RUN apt-get update && rm -rf /var/lib/apt/lists/*
COPY --from=build-stage /usr/local/cargo/bin/f1-notif-bot /usr/local/bin/f1-notif-bot
CMD ["f1-notif-bot"]