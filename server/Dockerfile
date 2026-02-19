FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
# examines project to construct a recipe to represent dependency build info
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG TARGETARCH
RUN case "${TARGETARCH}" in \
      amd64) echo "x86_64-unknown-linux-musl" ;; \
      arm64) echo "aarch64-unknown-linux-musl" ;; \
      *) echo "Unsupported: ${TARGETARCH}" && exit 1 ;; \
    esac > /rust_target.txt
COPY --from=planner /app/recipe.json recipe.json
# need musl to run with glibc statically linked on scratch image...
RUN apt-get update && apt-get install -y \
  musl-tools \
  pkg-config \
  libssl-dev \
  && rm -rf /var/lib/apt/lists/*
RUN rustup target add $(cat /rust_target.txt)
# build dependencies - this is caching layer!!!
RUN cargo chef cook --release --target $(cat /rust_target.txt) --recipe-path recipe.json
# build actual application
COPY . .
RUN cargo build --release --target $(cat /rust_target.txt) --bin server
RUN strip /app/target/$(cat /rust_target.txt)/release/server
# stash binary in predictable location for later stages
RUN cp /app/target/$(cat /rust_target.txt)/release/server /app/server

FROM builder AS development
# crash if our application has logic errors, also cache 'check's for cargo
# watch to avoid running the run script on breaking changes
RUN cargo check --release --target $(cat /rust_target.txt) --bin server
# install hot reload helper
RUN cargo install cargo-watch
# use --poll to watch for saved changes on host volume end...
# use check to ensure we SHOULD be running our application, and it isnt just
# saved in an intermediate state...
CMD cargo watch -w backend -w service \
  -x "check --release --target $(cat /rust_target.txt) --bin server" \
  --poll \
  -s "cargo run --release --target $(cat /rust_target.txt) --bin server"

FROM scratch AS production
WORKDIR /app
COPY --from=builder /app/server ./backend/server
ENTRYPOINT ["./backend/server"]
