FROM docker.io/library/rust:1.96-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/mythenheim /usr/local/bin/mythenheim
COPY examples/mythenheim.toml /etc/mythenheim/mythenheim.toml
USER nonroot:nonroot
EXPOSE 37171
ENTRYPOINT ["/usr/local/bin/mythenheim"]
CMD ["--config", "/etc/mythenheim/mythenheim.toml"]
