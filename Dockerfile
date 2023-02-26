FROM rust:1.67.1 as build-stage

WORKDIR /app

COPY . .

RUN  cargo install --path .

FROM debian:buster-slim as runner

RUN apt-get update && apt-get install glibc-source -y && rm -rf /var/lib/apt/lists/*
COPY --from=build-stage /usr/local/cargo/bin/f1-notif-bot /usr/local/bin/f1-notif-bot

RUN mkdir /config

CMD ["f1-notif-bot"]