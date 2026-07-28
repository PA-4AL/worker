# =============================================================================
# Worker Rust (import / export Excel) — build release puis image distroless.
# Contexte de build : la racine de ce repo (`docker build .`).
#
# Choix des images (justifications : infra/docs/DOCKER.md) :
#   - rust:1.97.1-slim-bookworm      → Docker Official Image, toolchain stable
#                                      épinglée au patch (build reproductible).
#   - distroless/cc-debian12:nonroot → image Google (maintenue et reconstruite
#                                      automatiquement) : glibc + libssl +
#                                      ca-certificates et rien d'autre. Aucun
#                                      shell → surface d'attaque minimale, et
#                                      exécution en UID 65532 non privilégié.
#     Variante `cc` (et non `static`) car la stack Pub/Sub passe par native-tls,
#     donc OpenSSL 3 en lien dynamique. Debian 12 des deux côtés = même OpenSSL
#     3.0, les .so sont donc compatibles entre les deux étages.
# =============================================================================

FROM rust:1.97.1-slim-bookworm AS build

# openssl-sys exige les en-têtes OpenSSL et pkg-config à la compilation.
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 1) Dépendances seules, compilées contre un main.rs vide : cette couche (la
#    plus lente) n'est invalidée que par Cargo.toml / Cargo.lock.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
 && echo 'fn main() {}' > src/main.rs \
 && cargo build --release --locked \
 && rm -rf src

# 2) Sources réelles : seul le crate du worker est recompilé.
COPY src ./src
RUN touch src/main.rs \
 && cargo build --release --locked \
 && strip target/release/worker


FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=build /app/target/release/worker /usr/local/bin/worker

# Cloud Run injecte PORT ; la sonde HTTP du worker (src/health.rs) l'utilise.
ENV PORT=8080
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/worker"]
