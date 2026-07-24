# syntax=docker/dockerfile:1
FROM rust:1.95.0-alpine3.22 AS build

RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM scratch
COPY --from=build /src/target/release/braintrust-lambda-extension /braintrust-lambda-extension
