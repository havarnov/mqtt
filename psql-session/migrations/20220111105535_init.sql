-- Add migration script here
CREATE TABLE sessions (
    id INT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    client_identifier TEXT NOT NULL,
    session_data JSONB NOT NULL DEFAULT '{}'
);
