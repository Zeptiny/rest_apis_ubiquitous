FROM dpage/pgadmin4:9.18

# Desktop mode: no pgAdmin login and no master password. For local development only.
ENV PGADMIN_DEFAULT_EMAIL=admin@example.com \
    PGADMIN_DEFAULT_PASSWORD=admin \
    PGADMIN_CONFIG_SERVER_MODE=False \
    PGADMIN_CONFIG_MASTER_PASSWORD_REQUIRED=False \
    # The default (::) fails on hosts with IPv6 disabled; the published port is IPv4 anyway.
    PGADMIN_LISTEN_ADDRESS=0.0.0.0

# Registers the Postgres container as a server on first start. libpq only reads
# a password file that is private to its owner, hence the explicit chown/chmod.
COPY servers.json /pgadmin4/servers.json
COPY --chown=pgadmin:root --chmod=600 pgpass /pgpass

# pgAdmin's gunicorn treats SIGTERM as a graceful shutdown that outlasts Docker's
# 10s stop timeout; SIGINT is its quick shutdown.
STOPSIGNAL SIGINT
