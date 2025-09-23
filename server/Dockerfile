FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

ARG RUST_TARGET
ENV RUST_TARGET=$RUST_TARGET

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# need musl to run with glibc statically linked on scratch image...
RUN apt-get update && apt-get install -y \
  musl-tools \
  pkg-config \
  libssl-dev \
  && rm -rf /var/lib/apt/lists/*
RUN rustup target add $RUST_TARGET
# build dependencies - this is caching layer!!!
RUN cargo chef cook --release --target $RUST_TARGET --recipe-path recipe.json
# build actual application
COPY . .
RUN cargo build --release --target $RUST_TARGET --bin server
RUN strip /app/target/$RUST_TARGET/release/server

FROM builder AS development
# crash if our application has logic errors, also cache 'check's for cargo
# watch to avoid running the run script on breaking changes
RUN cargo check --release --target $RUST_TARGET --bin server
# install hot reload helper
RUN cargo install cargo-watch
# use --poll to watch for saved changes on host volume end...
# use check to ensure we SHOULD be running our application, and it isnt just
# saved in an intermediate state...
CMD cargo watch -w backend -w service \
  -x "check --release --target $RUST_TARGET --bin server" \
  --poll \
  -s "cargo run --release --target $RUST_TARGET --bin server"

FROM scratch AS production
# renew args
ARG RUST_TARGET
ENV RUST_TARGET=$RUST_TARGET
# move over app
WORKDIR /app
COPY --from=builder /app/target/$RUST_TARGET/release/server ./backend/server
ENTRYPOINT ["./backend/server"]