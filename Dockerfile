FROM rust:latest as build

WORKDIR /

RUN cargo build --release

FROM alpine:latest

COPY --from=build /target/release/f1-notif-bot /bin/f1-notif-bot

CMD ["f1-notif-bot"]
