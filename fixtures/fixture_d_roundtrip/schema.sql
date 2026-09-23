CREATE TABLE items (
    id SERIAL PRIMARY KEY,
    value NUMERIC(10, 2) NOT NULL
);

CREATE TABLE notes (
    id INTEGER PRIMARY KEY,
    body TEXT
);

CREATE UNIQUE INDEX notes_body_exact_unique ON notes(body);
CREATE INDEX notes_body_lower_lookup ON notes(lower(body));
