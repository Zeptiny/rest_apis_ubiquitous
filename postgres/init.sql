-- Runs once, when the Postgres data volume is first initialised.
CREATE TABLE users (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        TEXT        NOT NULL,
    email       TEXT        NOT NULL CONSTRAINT users_email_key UNIQUE,
    birthday    DATE        NOT NULL,
    cpf         CHAR(11)    NOT NULL CONSTRAINT users_cpf_key UNIQUE,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
