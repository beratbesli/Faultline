ALTER TABLE users ADD COLUMN normalized_email TEXT;
UPDATE users SET normalized_email = LOWER(email);
CREATE UNIQUE INDEX users_normalized_email_unique ON users(normalized_email);
