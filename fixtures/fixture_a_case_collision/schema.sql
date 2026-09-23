CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email TEXT NOT NULL
);

CREATE UNIQUE INDEX users_email_exact_unique ON users(email);
CREATE INDEX users_email_lower_lookup ON users(lower(email));
