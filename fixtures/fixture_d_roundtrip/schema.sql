CREATE TABLE items (
    id SERIAL PRIMARY KEY,
    value NUMERIC(10, 2) NOT NULL
);

CREATE TABLE notes (
    id INTEGER PRIMARY KEY,
    body TEXT
);
