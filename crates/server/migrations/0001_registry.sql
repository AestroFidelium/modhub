-- Mods and their published releases.
--
-- Releases are immutable: a version, once published, is never replaced, so a server
-- that pinned `cloth 1.2.0` always gets the same bytes. Hence UNIQUE (mod_id, version)
-- and no UPDATE path. The module is stored with its SHA-256, checked on every read. Versions are stored as canonical semver text (one spelling per
-- version, enforced by ModVersion); precedence ordering happens in Rust.

CREATE TABLE mods (
    id   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- Mirrors ModName: the database refuses what the domain refuses, even for writes
    -- that bypass the application.
    name TEXT NOT NULL
        CONSTRAINT mods_name_key UNIQUE
        CONSTRAINT mods_name_rule CHECK (
            octet_length(name) <= 64 AND name ~ '^[a-z][a-z0-9]*(-[a-z0-9]+)*$'
        )
);

CREATE TABLE releases (
    id      BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    mod_id  BIGINT NOT NULL REFERENCES mods (id) ON DELETE RESTRICT,
    version TEXT NOT NULL,
    -- The checked .wasm (at most 8 MiB), in the row: one transaction publishes it.
    module  BYTEA NOT NULL,
    sha256  BYTEA NOT NULL CONSTRAINT releases_sha256_len CHECK (octet_length(sha256) = 32),
    CONSTRAINT releases_mod_version_key UNIQUE (mod_id, version)
);
