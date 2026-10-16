FROM postgres:18-alpine

# Demo credentials. Keep them in sync with DATABASE_URL in docker-compose.yml
# and with pgadmin/pgpass.
ENV POSTGRES_DB=users \
    POSTGRES_USER=api \
    POSTGRES_PASSWORD=api

# Creates the schema the first time the data volume is initialised.
COPY init.sql /docker-entrypoint-initdb.d/

# Checks over TCP on purpose: while init.sql runs, the temporary server only
# listens on a Unix socket, so a socket check would report "ready" too early.
HEALTHCHECK --interval=2s --timeout=5s --retries=15 \
    CMD pg_isready -h 127.0.0.1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" || exit 1
